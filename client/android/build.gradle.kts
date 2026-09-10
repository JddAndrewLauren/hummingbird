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

// ---------------------------------------------------------------------------
// The build version, shared by every application module (`:app`, `:wear`)
// so the two APKs a deploy produces carry the same stamp. The scheme is the
// web's (`client/web/src/shell/build-version.ts`): `VERSION` at the repo
// root plus the commits since it was last touched, as the patch;
// `versionCode` is the total commit count, so it only ever climbs — Android
// refuses a downgrade, and a stable, climbing code is what lets a device
// install a new APK over the old one in place (`deploy.sh`). Both fall back
// when git cannot say (CI's checkout is shallow, and an export has no
// history at all): `1` and `+unknown`, never a number nobody wrote. `git`
// is run at configure time, twice, against the repo root; a non-zero exit
// or a shallow clone yields the fallback rather than a truncated count.
// Read as `rootProject.extra["hbVersionCode"]` / `["hbVersionName"]`.
// ---------------------------------------------------------------------------
fun git(vararg args: String): String? =
    runCatching {
        val proc = ProcessBuilder("git", *args)
            .directory(projectDir)
            .redirectErrorStream(true)
            .start()
        val out = proc.inputStream.bufferedReader().readText().trim()
        if (proc.waitFor() == 0) out else null
    }.getOrNull()

val gitIsShallow: Boolean = git("rev-parse", "--is-shallow-repository") != "false"

fun buildVersionCode(): Int =
    if (gitIsShallow) 1 else git("rev-list", "--count", "HEAD")?.toIntOrNull() ?: 1

fun buildVersionName(): String {
    val versionFile = repoRoot.resolve("VERSION")
    val base = Regex("""^\s*(\d+)\.(\d+)\.(\d+)\s*$""")
        .find(runCatching { versionFile.readText() }.getOrDefault(""))
        ?: return "0.0.0+unknown"
    val (major, minor, patch) = base.destructured
    if (gitIsShallow) return "$major.$minor.$patch+unknown"
    val touched = git("log", "-1", "--format=%H", "--", versionFile.absolutePath)
        ?.takeIf { it.isNotEmpty() } ?: return "$major.$minor.$patch+unknown"
    val since = git("rev-list", "--count", "$touched..HEAD")?.toIntOrNull()
        ?: return "$major.$minor.$patch+unknown"
    return "$major.$minor.${patch.toInt() + since}"
}

extra["hbVersionCode"] = buildVersionCode()
extra["hbVersionName"] = buildVersionName()

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
