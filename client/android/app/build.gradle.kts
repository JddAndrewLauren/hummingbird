import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    // Reads `google-services.json` (committed alongside this line) and
    // generates the resources `FirebaseApp` initialises itself from. Hard
    // requirement: this plugin *fails the build* if the json is absent,
    // which is why the two are one commit and why the json is not
    // gitignored — see the `dependencies` note below.
    alias(libs.plugins.google.services)
}

// The `client/` cargo workspace, one level above this Gradle root — the
// Rust side of the two seam tasks below.
val cargoWorkspace: File = rootProject.projectDir.parentFile

// `cargo` and `cargo-ndk` live in ~/.cargo/bin, which Android Studio's
// GUI-launched Gradle daemon does not have on PATH.
val cargoPath: String =
    System.getenv("PATH") + File.pathSeparator +
        "${System.getProperty("user.home")}/.cargo/bin"

// ---------------------------------------------------------------------------
// Seam task 1: cross-compile hummingbird-ffi-mobile into jniLibs.
// arm64-v8a is the device (Pixel 10 Pro Fold); x86_64 is the emulator.
// ---------------------------------------------------------------------------
val cargoNdkBuild = tasks.register<Exec>("cargoNdkBuild") {
    group = "rust"
    description = "cargo-ndk cross-compile of hummingbird-ffi-mobile into src/main/jniLibs"
    workingDir = cargoWorkspace
    environment("PATH", cargoPath)
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-o", layout.projectDirectory.dir("src/main/jniLibs").asFile.absolutePath,
        "build", "--release", "-p", "hummingbird-ffi-mobile",
    )
}

// ---------------------------------------------------------------------------
// Seam task 2: generate the Kotlin binding from the host-built cdylib
// (UniFFI library mode: the exported surface in ffi-mobile/src/lib.rs is
// the single source of truth; no .udl). Two steps — build the host dylib,
// then run the bindgen bin against it.
// ---------------------------------------------------------------------------
val cargoHostBuild = tasks.register<Exec>("cargoHostBuild") {
    group = "rust"
    description = "Host build of hummingbird-ffi-mobile for uniffi-bindgen library mode"
    workingDir = cargoWorkspace
    environment("PATH", cargoPath)
    commandLine("cargo", "build", "-p", "hummingbird-ffi-mobile")
}

val hostCdylibName: String =
    when {
        System.getProperty("os.name").lowercase().contains("mac") -> "libhummingbird_ffi_mobile.dylib"
        // The repo's Windows story is WSL (memory: never /mnt/c), so the
        // remaining native case is Linux — CI's ubuntu runner included.
        else -> "libhummingbird_ffi_mobile.so"
    }

val generateUniffiBindings = tasks.register<Exec>("generateUniffiBindings") {
    group = "rust"
    description = "uniffi-bindgen Kotlin binding into build/generated/uniffi"
    dependsOn(cargoHostBuild)
    workingDir = cargoWorkspace
    environment("PATH", cargoPath)
    commandLine(
        "cargo", "run", "-p", "hummingbird-ffi-mobile",
        "--features", "bindgen", "--bin", "uniffi-bindgen", "--",
        "generate",
        "--library", "target/debug/$hostCdylibName",
        "--language", "kotlin",
        "--out-dir", layout.buildDirectory.dir("generated/uniffi").get().asFile.absolutePath,
    )
}

android {
    namespace = "net.twinion.hummingbird"
    compileSdk = 36

    defaultConfig {
        applicationId = "net.twinion.hummingbird"
        // minSdk 35: the Pixel 10 Pro Fold is the only install target
        // (grilling 2026-08-14 on #141); nothing older is ever sideloaded.
        minSdk = 35
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0-m0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // The authority's origin (ADR-0008), host-supplied to the core at
        // init per ADR-0003 — the app is the host, so it lives here.
        buildConfigField("String", "AUTHORITY_BASE_URL", "\"https://hb.twinion.net\"")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Release signing reads an operator-local keystore.properties
            // (never committed, never in Actions — the signing key follows
            // the ADMIN_SECRET handling rule; see CLAUDE.md's credential
            // blast-radius section and the grilling decision on #141).
            val keystoreProperties = rootProject.file("keystore.properties")
            if (keystoreProperties.exists()) {
                val props = Properties().apply { keystoreProperties.inputStream().use { load(it) } }
                signingConfigs.create("release") {
                    storeFile = rootProject.file(props.getProperty("storeFile"))
                    storePassword = props.getProperty("storePassword")
                    keyAlias = props.getProperty("keyAlias")
                    keyPassword = props.getProperty("keyPassword")
                }
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    sourceSets {
        getByName("main") {
            java.srcDir(layout.buildDirectory.dir("generated/uniffi"))
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        // The generated binding's JNA `.so` and ours coexist; nothing to
        // exclude yet — kept as the place ABI packaging decisions land.
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

tasks.named("preBuild") {
    dependsOn(cargoNdkBuild, generateUniffiBindings)
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.ktx)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.coroutines.android)
    implementation(libs.work.runtime.ktx)
    implementation(libs.security.crypto)
    implementation(libs.androidx.navigation.compose)
    implementation(libs.androidx.core.ktx)
    // M2/#141's push half. The BoM versions `firebase-messaging` below.
    //
    // **`google-services.json` is committed, deliberately.** It is not a
    // credential in the sense CLAUDE.md's blast-radius rule means: every
    // value in it (project id, app id, API key) is embedded in every APK,
    // including the debug artifact `android.yml` already publishes, so
    // committing it discloses nothing a build consumer does not hold. The
    // credential that *can* send is `FCM_SERVICE_ACCOUNT`, which stays a
    // Worker secret (ADR-0011). The deciding factor was CI: the plugin
    // above fails the build without the json, so gitignoring it would put
    // `:app:assembleDebug` permanently red or force a placeholder-json
    // step, buying build complexity for no secrecy.
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    implementation("${libs.jna.get()}@aar")

    debugImplementation(libs.compose.ui.tooling)

    // M4/#538: the skills runner lane's transport. The lane is physically
    // unable to reach the sync engine (`SkillsLaneIsolationTest`), so this
    // is a second, deliberately isolated way out of the process — a skill
    // request is a question, and questions go stale (#269); a sync mutation
    // is a fact the user already decided.
    implementation(libs.okhttp)

    testImplementation(libs.junit)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.coroutines.test)

    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}

tasks.matching { it.name.startsWith("merge") && it.name.contains("AndroidTestAssets") }
    .configureEach { dependsOn(copySkillsFixture) }

// ColorTokenDriftTest reads tokens/colors.css and Color.kt from the repo
// root — two levels above this Gradle root (client/android → client → repo).
// A system property rather than `user.dir`, which differs between Gradle
// and an IDE runner.
val repoRoot: File = rootProject.projectDir.parentFile.parentFile
tasks.withType<Test>().configureEach {
    systemProperty("hummingbird.repoRoot", repoRoot.absolutePath)
    // The CSS sits outside this Gradle project, so without this line a
    // token change leaves testDebugUnitTest UP-TO-DATE and the drift gate
    // silently doesn't rerun — a stale local green. (CI runs fresh either
    // way; this is for the local loop.)
    inputs.file(File(repoRoot, ".claude/skills/hummingbird-design/tokens/colors.css"))
        .withPropertyName("designTokensCss")
    // Same freshness fix for TypeTokenDriftTest (#528): fonts.css sits
    // outside this Gradle project too.
    inputs.file(File(repoRoot, ".claude/skills/hummingbird-design/tokens/fonts.css"))
        .withPropertyName("designTokensFontsCss")
    // And the launcher backgrounds the colour gate now covers. This one is
    // in-tree, but res/ is not an input to the unit-test task either, so
    // editing a hex here leaves the gate UP-TO-DATE just the same.
    inputs.file(File(repoRoot, "client/android/app/src/main/res/values/colors.xml"))
        .withPropertyName("launcherColorsXml")
}

// ---------------------------------------------------------------------------
// M4/#538: the shared run-body fixture, copied into androidTest assets.
//
// `client/core/tests/fixtures/skills-run-bodies.json` is read directly off
// disk by the Rust and TypeScript sides. The instrumented suite cannot do
// that — it runs on the device, where the repo does not exist — and a
// hand-typed copy in `assets/` would be exactly the drift the fixture exists
// to prevent. So Gradle copies the real file in at build time: one source of
// truth, three readers, and a stale copy is impossible because the copy is
// generated. (The JVM suite needs no copy — it pins that the lane posts the
// core's string verbatim, not what the bytes are.)
// ---------------------------------------------------------------------------
val skillsFixture: File = File(repoRoot, "client/core/tests/fixtures/skills-run-bodies.json")
val copySkillsFixture = tasks.register<Copy>("copySkillsRunBodyFixture") {
    group = "verification"
    description = "Copy the shared run-body fixture into androidTest assets (#538)"
    from(skillsFixture)
    into(layout.buildDirectory.dir("generated/skillsFixture/assets"))
}
android.sourceSets.getByName("androidTest") {
    assets.srcDir(layout.buildDirectory.dir("generated/skillsFixture/assets"))
}
tasks.matching { it.name.startsWith("generate") && it.name.contains("AndroidTestAssets") }
    .configureEach { dependsOn(copySkillsFixture) }
