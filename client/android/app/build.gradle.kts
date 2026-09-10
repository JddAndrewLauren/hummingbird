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

android {
    namespace = "net.twinion.hummingbird"
    compileSdk = 36

    defaultConfig {
        applicationId = "net.twinion.hummingbird"
        // minSdk 35: the Pixel 10 Pro Fold is the only install target
        // (grilling 2026-08-14 on #141); nothing older is ever sideloaded.
        minSdk = 35
        targetSdk = 36
        // The build version: computed once in the root build file (its
        // header has the scheme) and shared with `:wear`, so both APKs a
        // deploy produces carry the same stamp.
        versionCode = rootProject.extra["hbVersionCode"] as Int
        versionName = rootProject.extra["hbVersionName"] as String
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
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

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    testOptions {
        // #576: Robolectric needs the merged resources/manifest on the unit
        // test classpath, or `createComposeRule()` cannot inflate anything.
        unitTests.isIncludeAndroidResources = true
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

// Without `keystore.properties` the release build type has no signing config
// and AGP quietly emits an *unsigned* APK, which no phone will install and
// which `deploy.sh` would then ship. Refuse at packaging time instead; the
// debug lane (CI's whole lane) never reaches this task.
tasks.matching { it.name == "packageRelease" }.configureEach {
    doFirst {
        if (!rootProject.file("keystore.properties").exists()) {
            throw GradleException(
                "client/android/keystore.properties is missing: a release APK would be unsigned. " +
                    "Run client/android/deploy.sh, which writes it from 1Password for the build."
            )
        }
    }
}

dependencies {
    // The Rust seam and the host core package — `CoreHolder`, `TokenStore`,
    // the diagnostics recorder, `SyncWorker` — shared with every other
    // device app (ADR-0039). JNA, coroutines and WorkManager arrive
    // transitively as that module's `api`.
    implementation(project(":core-binding"))
    // The brand: design tokens, the bundled typefaces, the Lucide drawables
    // (`net.twinion.hummingbird.brand.R`), and the pane words every device
    // says identically (ADR-0039).
    implementation(project(":brand"))
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
    // M3/#530: the frontier board's axis/facet/collapse preferences.
    implementation(libs.androidx.datastore.preferences)
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

    debugImplementation(libs.compose.ui.tooling)

    // M4/#538: the skills runner lane's transport. The lane is physically
    // unable to reach the sync engine (`SkillsLaneIsolationTest`), so this
    // is a second, deliberately isolated way out of the process — a skill
    // request is a question, and questions go stale (#269); a sync mutation
    // is a fact the user already decided.
    implementation(libs.okhttp)

    // ADR-0039: the watch's token hand-off. Settings' Watch card sends the
    // `device-watch` token over the Wearable Data Layer (`WatchTokenSender`);
    // `Task.await()` is the coroutines bridge for those calls.
    implementation(libs.play.services.wearable)
    implementation(libs.coroutines.play.services)

    testImplementation(libs.junit)
    testImplementation(libs.okhttp.mockwebserver)
    testImplementation(libs.coroutines.test)
    // The plain (non-`@aar`) JNA artifact, for `testDebugUnitTest` alone:
    // tests here that reach a real uniffi call (`GlyphRenderTest` and kin,
    // through the binding `:core-binding` exposes) need the host's own
    // native dispatch library on the JVM classpath, which only this plain
    // jar carries; the `@aar` variant the app ships bundles Android `.so`s
    // only. Where the host cdylib itself is found is the root build file.
    testImplementation(libs.jna)
    // #576: the width-measuring gate. `ui-test-junit4` brings
    // `createComposeRule()`; Robolectric is what lets it run without an
    // emulator, and `ui-test-manifest` supplies the empty activity the rule
    // launches into (a debug-only manifest merge, hence `debugImplementation`).
    testImplementation(libs.robolectric)
    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.compose.ui.test.junit4)
    debugImplementation(libs.compose.ui.test.manifest)

    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.test.runner)
}

tasks.matching { it.name.startsWith("merge") && it.name.contains("AndroidTestAssets") }
    .configureEach { dependsOn(copySkillsFixture) }

// The repo root, two levels above this Gradle root (client/android → client
// → repo), for the androidTest fixture copy below. The JVM unit-test
// configuration that used to sit here (`hummingbird.repoRoot`,
// `jna.library.path`, the drift gates' input files) is the root build
// file's `subprojects` block now, shared with `:core-binding`.
val repoRoot: File = rootProject.projectDir.parentFile.parentFile

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
