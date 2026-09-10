plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

// ---------------------------------------------------------------------------
// `:brand` — the Hummingbird Design System as every device app draws it
// (ADR-0026's hand-port, ADR-0039's second consumer). What ships from here:
// the token constants (`ui/theme/Color.kt`), the bundled typefaces
// (`ui/theme/Font.kt`, `res/font/`, OFL licences under `licenses/fonts/`),
// every vendored Lucide drawable (`res/drawable/ic_*.xml`), and the pane
// words every device says identically — `PaneAnswers.kt`, `PaneGlyph.kt`,
// `PaneCollapse.kt`, `PaneBand.kt`, `NowPaneWords.kt`. Packages are
// unchanged from when these lived in `:app`
// (`net.twinion.hummingbird.ui.{theme,panes}`); the R class is this
// module's own, `net.twinion.hummingbird.brand.R`, because R classes are
// non-transitive in this build (`gradle.properties`).
//
// **Nothing here is a composable.** `Color`, `FontFamily` and `Int`
// resource ids are the whole Compose footprint, so this module applies no
// Compose compiler plugin; the Material3 theme and every screen are the
// application modules' own. Launcher art (`ic_launcher_*`, the adaptive
// icon's colours in `res/values/colors.xml`) is per-app and stays in `:app`.
//
// The drift gates (`ColorTokenDriftTest`, `TypeTokenDriftTest`) live here
// with their subjects and read the design mirror through the root build
// file's hoisted `Test` configuration.
// ---------------------------------------------------------------------------

android {
    namespace = "net.twinion.hummingbird.brand"
    compileSdk = 36

    defaultConfig {
        // Wear OS 5's floor, as `:core-binding`; `:app` stays 35.
        minSdk = 34
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    // `PaneAnswers`/`PaneCollapse`/`PaneBand` take the seam's own types
    // (`MobileRankedPane`, `MobilePaneBand`, …) in their public signatures.
    api(project(":core-binding"))
    // `Color`, `FontFamily`: the two Compose types in this module's surface.
    api(platform(libs.compose.bom))
    api(libs.compose.ui)

    testImplementation(libs.junit)
}
