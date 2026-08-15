// The native Android client (#141): a standalone Gradle build rooted here,
// deliberately NOT coupled into any other build system in the repo — the
// only seam to the Rust side is the two cargo invocations app/build.gradle.kts
// wires (cargo-ndk for the .so, uniffi-bindgen for the Kotlin binding),
// both running against the `client/` cargo workspace one directory up.
pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "hummingbird-android"
include(":app")
