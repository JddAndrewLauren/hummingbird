plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.google.services) apply false
}

// ---------------------------------------------------------------------------
// The JVM unit-test configuration every module shares. It lived in
// `app/build.gradle.kts` while `:app` was the only module; `:core-binding`
// (ADR-0039) split the tests that reach the Rust seam across two modules,
// and a test's reach outside its own Gradle project is the same wherever it
// runs — so it is stated once, here, for all of them.
// ---------------------------------------------------------------------------

// The repo root: two levels above this Gradle root (client/android → client
// → repo). A system property rather than `user.dir`, which differs between
// Gradle and an IDE runner. The drift gates (`ColorTokenDriftTest` and its
// kin) read design tokens and web sources from here.
val repoRoot: File = projectDir.parentFile.parentFile

// The `client/` cargo workspace: `:core-binding`'s `cargoHostBuild` writes
// the host cdylib under its `target/debug`.
val cargoWorkspace: File = projectDir.parentFile

subprojects {
    tasks.withType<Test>().configureEach {
        systemProperty("hummingbird.repoRoot", repoRoot.absolutePath)
        // #537: `cargoHostBuild`'s own output dir, so a JVM unit test that
        // reaches a real uniffi call (`ZoneBridgeTest`) can load the host
        // cdylib JNA dlopens against. `:core-binding`'s
        // `generateUniffiBindings` depends on `cargoHostBuild` already, so
        // the file exists before any module's Kotlin compiles; the explicit
        // edge here is for a test task invoked on its own.
        dependsOn(":core-binding:cargoHostBuild")
        systemProperty("jna.library.path", File(cargoWorkspace, "target/debug").absolutePath)
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
        // Same freshness fix for BottomNavStructuralTest (#532): nav-bar.ts
        // sits outside this Gradle project too, and a change to the web's
        // ON_THE_BAR set would otherwise leave the bar-set pin UP-TO-DATE.
        inputs.file(File(repoRoot, "client/web/src/shell/nav-bar.ts"))
            .withPropertyName("navBarTs")
        // Same fix for WindowWidthStructuralTest and FrontierLanesTest (the
        // unfolded slice): both pin against web shell sources that sit outside
        // this Gradle project.
        inputs.file(File(repoRoot, "client/web/src/shell/breakpoints.ts"))
            .withPropertyName("breakpointsTs")
        inputs.file(File(repoRoot, "client/web/src/screens/frontier-lanes.ts"))
            .withPropertyName("frontierLanesTs")
        // Same freshness fix for DiagnosticsRecorderTest's forbidden-field drift
        // gate (#741): diagnostics.rs sits outside this Gradle project too, so a
        // change to FORBIDDEN_FIELD_NAMES would otherwise leave the gate
        // UP-TO-DATE.
        inputs.file(File(repoRoot, "server/domain/src/diagnostics.rs"))
            .withPropertyName("domainDiagnosticsRs")
    }
}
