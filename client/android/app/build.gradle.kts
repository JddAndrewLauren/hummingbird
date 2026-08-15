import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
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
    implementation(libs.coroutines.android)
    implementation(libs.work.runtime.ktx)
    implementation(libs.security.crypto)
    implementation("${libs.jna.get()}@aar")

    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)

    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}

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
}
