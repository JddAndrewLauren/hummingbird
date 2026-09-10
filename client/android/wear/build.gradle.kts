import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// ---------------------------------------------------------------------------
// `:wear` — the Pixel Watch client (ADR-0039): a device with its own Rust
// core, its own `device` token (`device-watch`) and its own APK, sharing
// `:core-binding` (the seam and the host core package) and `:brand` (tokens,
// type, glyphs, the pane words) with the phone and nothing else. No push
// lane, no notifications, no Firebase: the watch syncs on open, on a
// 60-second foreground cadence, and on WorkManager's hourly leg.
//
// **The `applicationId` is the phone's, on purpose.** The Wearable Data
// Layer delivers a message only between apps that share a package name AND
// a signing certificate on paired nodes; the token the phone sends
// (`WatchTokenSender` → `TokenListenerService`) rides that channel, so a
// watch build under another id, or under the debug key while the phone
// runs the release key, receives nothing — silently. `deploy.sh` builds
// both APKs under the one release key for exactly this reason, and prints
// both certificate lines so a mismatch is visible before the install.
// ---------------------------------------------------------------------------

android {
    namespace = "net.twinion.hummingbird.wear"
    compileSdk = 36

    defaultConfig {
        applicationId = "net.twinion.hummingbird"
        // Wear OS 5 is API 34 and no Wear release is 35, so the watch's
        // floor is 34 — the reason `:core-binding` and `:brand` sit there too.
        minSdk = 34
        targetSdk = 36
        // Computed once in the root build file, shared with `:app`, so both
        // APKs a deploy produces carry the same stamp.
        versionCode = rootProject.extra["hbVersionCode"] as Int
        versionName = rootProject.extra["hbVersionName"] as String
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // The Pixel Watch 4 runs a 32-bit userspace — `ro.product.cpu.abilist`
        // is `armeabi-v7a,armeabi`, no arm64 at all — so the arm64-only APK
        // this shipped as first answered INSTALL_FAILED_NO_MATCHING_ABIS on
        // hardware (2026-09-10). armeabi-v7a is what the watch installs;
        // arm64-v8a stays in for the arm64 Wear AVD and any 64-bit watch.
        // `:core-binding` still cross-compiles x86_64 for the phone
        // emulator; this filter drops it at packaging. A Wear emulator on an
        // Intel host (x86_64) would need this widened — a `-PwearEmulator`
        // flag, documented in the README, deliberately not built until
        // someone needs it.
        ndk {
            abiFilters += listOf("armeabi-v7a", "arm64-v8a")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Release signing reads the same operator-local keystore.properties
            // as `:app` — the SAME key, necessarily (see the header). Copied
            // from `app/build.gradle.kts` rather than hoisted: the block lives
            // inside AGP's `android {}` DSL, and a root-level version would
            // have to reach into each module's `signingConfigs` by name; two
            // eleven-line copies with a pointer each way read more honestly.
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
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

// The same refusal `app/build.gradle.kts` carries, for the same reason: with
// no `keystore.properties` AGP quietly emits an *unsigned* release APK, and
// an unsigned watch APK is worse than useless — it installs nowhere, and a
// differently-keyed one would install and then never receive the token.
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
    implementation(project(":core-binding"))
    implementation(project(":brand"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.wear.compose.material3)
    implementation(libs.wear.compose.foundation)
    implementation(libs.wear.compose.navigation)
    // The system input chooser capture hands its one line to.
    implementation(libs.wear.input)
    // "Open on phone" from an Items row: `RemoteActivityHelper` carries the
    // item link (`ItemLink`, `:core-binding`) to the paired phone.
    implementation(libs.wear.remote.interactions)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.androidx.core.ktx)
    // The token hand-off's receiving end: `WearableListenerService`.
    implementation(libs.play.services.wearable)
    // The capture tile: `TileService`, the ProtoLayout builders it renders
    // with, and the future adapter it answers through.
    implementation(libs.wear.tiles)
    implementation(libs.wear.protolayout)
    implementation(libs.wear.protolayout.material3)
    implementation(libs.androidx.concurrent.futures)

    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
}
