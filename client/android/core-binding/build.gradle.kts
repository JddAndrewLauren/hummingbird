plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

// ---------------------------------------------------------------------------
// `:core-binding` — the Rust seam and the host-side core package, as one
// Android library every hummingbird device app consumes (ADR-0039: the
// phone `:app` and the watch `:wear`). This module is *the* place the two
// cargo invocations live; nothing else in this Gradle build runs cargo, so
// a second consumer costs no second `.so` build and cannot drift from the
// first. What ships from here: the cross-compiled `libhummingbird_ffi_mobile`
// per ABI, the UniFFI-generated Kotlin binding, and the Kotlin that every
// host needs identically — `CoreHolder` (the one core per process),
// `TokenStore`, `TokenValidation`, `ZoneBridge`, `SyncHistoryStore`,
// `WallClock`, the diagnostics recorder and journal, and `SyncWorker`.
// Packages are unchanged from when these lived in `:app`
// (`net.twinion.hummingbird.{core,diagnostics,sync}`); only the module moved.
//
// What is deliberately NOT here: anything that draws (Compose, Material3,
// the theme), anything that only one device does (push, notifications,
// `NetworkMonitor`), and the release-signing block — those stay in each
// application module.
// ---------------------------------------------------------------------------

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
// arm64-v8a is every device (Pixel 10 Pro Fold, Pixel Watch); x86_64 is the
// phone emulator. Both are built here once; an application module that
// wants fewer (`:wear`) narrows with `abiFilters` at packaging time rather
// than with a second cargo task.
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
    namespace = "net.twinion.hummingbird.core"
    compileSdk = 36

    defaultConfig {
        // minSdk 34, not `:app`'s 35: Wear OS 5 is API 34 and no Wear
        // release is 35, so the library sits at the lowest consumer's
        // floor. The phone's own floor stays stated in `:app`.
        minSdk = 34
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        // The authority's origin (ADR-0008), host-supplied to the core at
        // init per ADR-0003. Every device app is a host of the same
        // authority, so the constant lives in the module they share —
        // `net.twinion.hummingbird.core.BuildConfig.AUTHORITY_BASE_URL`.
        buildConfigField("String", "AUTHORITY_BASE_URL", "\"https://hb.twinion.net\"")
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
        buildConfig = true
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
    // `api`, not `implementation`, for the three that appear in this
    // module's public surface: the generated binding's own types stand on
    // JNA, every core door is a `suspend` fun, and `SyncWorker` hands
    // consumers a `OneTimeWorkRequest`. `@aar` packaging is what ships the
    // Android-native JNI dispatch library.
    api("${libs.jna.get()}@aar")
    api(libs.coroutines.android)
    api(libs.work.runtime.ktx)
    // `TokenStore`'s `EncryptedSharedPreferences`; no consumer sees the type.
    implementation(libs.security.crypto)

    testImplementation(libs.junit)
    // The plain (non-`@aar`) JNA artifact, for `testDebugUnitTest` alone:
    // the `@aar` variant above bundles only Android per-ABI `.so`s, so a
    // JVM unit test that reaches a real uniffi call (`ZoneBridgeTest`,
    // #537) needs the host's own native dispatch library
    // (`libjnidispatch.jnilib` on macOS) on its classpath, which only this
    // plain jar carries. Where the host cdylib itself is found is the root
    // build file's hoisted `Test` configuration (`jna.library.path`).
    testImplementation(libs.jna)
}
