
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

// The harness dir is the single source of truth for the three shared files. They are
// COPIED into assets at build time rather than committed here, so the app can never
// drift from what run_hosted.py fed the hosted side. PromptAssemblyTest re-asserts the
// byte-identity anyway — belt and braces, because a silent drift here would invalidate
// the whole bake-off.
val harnessDir = rootProject.projectDir.parentFile
val harnessFiles = listOf("corpus.jsonl", "prompt.md", "schema.json")
val harnessAssets = layout.projectDirectory.dir("src/main/assets/harness")

val copyHarnessAssets = tasks.register<Copy>("copyHarnessAssets") {
    harnessFiles.forEach { name ->
        val src = File(harnessDir, name)
        if (!src.isFile) {
            throw GradleException("harness file missing: $src (build must not proceed without it)")
        }
        from(src)
    }
    into(harnessAssets)
    // Byte-preserving: no filtering, no line-ending translation, no expansion.
    duplicatesStrategy = DuplicatesStrategy.FAIL
}

fun git(vararg args: String): String {
    val result = providers.exec {
        workingDir = harnessDir
        commandLine("git", *args)
        isIgnoreExitValue = true
    }
    return runCatching { result.standardOutput.asText.get().trim() }.getOrDefault("")
}

// Which commit of the harness these results came from — recorded in nano_run_meta.json
// and shown on screen, so a result file is always traceable to a corpus/prompt/schema.
// `-dirty` tracks the three SHARED files only: uncommitted app source doesn't change
// what the model was asked, but an uncommitted corpus or prompt edit does.
val harnessCommit: String by lazy {
    val sha = git("rev-parse", "--short", "HEAD").ifEmpty { "unknown" }
    val dirty = git("status", "--porcelain", "--", *harnessFiles.toTypedArray()).isNotEmpty()
    if (dirty) "$sha-dirty" else sha
}

android {
    namespace = "net.twinion.nanorunner"
    compileSdk = 36

    defaultConfig {
        applicationId = "net.twinion.nanorunner"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
        buildConfigField("String", "HARNESS_COMMIT", "\"$harnessCommit\"")
        buildConfigField(
            "String",
            "MLKIT_GENAI_PROMPT_VERSION",
            "\"${libs.versions.genaiPrompt.get()}\"",
        )
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    buildFeatures {
        buildConfig = true
        viewBinding = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The golden test reads the copied assets and the ../ originals off the filesystem,
    // so unit tests need the copy to have happened and need to know where both live.
    testOptions {
        unitTests.all {
            it.systemProperty("harness.dir", harnessDir.absolutePath)
            it.systemProperty("harness.assets", harnessAssets.asFile.absolutePath)
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

tasks.named("preBuild") { dependsOn(copyHarnessAssets) }
tasks.withType<Test>().configureEach { dependsOn(copyHarnessAssets) }

dependencies {
    implementation(libs.mlkit.genai.prompt)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.serialization.json)
    testImplementation(libs.kotlinx.coroutines.test)
}
