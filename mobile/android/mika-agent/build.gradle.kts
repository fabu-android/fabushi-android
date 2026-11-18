plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val resolvedVersionName: String =
    (project.findProperty("versionName")?.toString()?.trim()?.takeIf { it.isNotEmpty() }) ?: "2.0.0"
val resolvedVersionCode: Int =
    (project.findProperty("versionCode")?.toString()?.trim()?.takeIf { it.isNotEmpty() })
        ?.let { raw ->
            raw.toIntOrNull()?.takeIf { it > 0 }
                ?: throw GradleException("Invalid -PversionCode='$raw': must be a positive 32-bit integer")
        } ?: 20000
val resolvedGitHash: String =
    (project.findProperty("gitHash")?.toString()?.trim()?.takeIf { it.isNotEmpty() })
        ?: "v$resolvedVersionName"

fun escapeJavaBuildConfigString(value: String): String =
    buildString(value.length + 2) {
        append('"')
        for (ch in value) {
            when (ch) {
                '\\' -> append("\\\\")
                '"' -> append("\\\"")
                '\n' -> append("\\n")
                '\r' -> append("\\r")
                '\t' -> append("\\t")
                else -> append(ch)
            }
        }
        append('"')
    }

android {
    namespace = "com.wangzan101.mika"
    compileSdk = 37

    defaultConfig {
        minSdk = 29
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
        buildConfigField("String", "VERSION_NAME", escapeJavaBuildConfigString(resolvedVersionName))
        buildConfigField("int", "VERSION_CODE", resolvedVersionCode.toString())
        buildConfigField("String", "GIT_HASH", escapeJavaBuildConfigString(resolvedGitHash))
        buildConfigField("boolean", "BETA", "false")
        buildConfigField("boolean", "ADB_LOGS", "false")
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }

    androidResources {
        noCompress += "xz"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        aidl = true
        buildConfig = true
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    api(project(":mika-core"))

    implementation("com.google.protobuf:protobuf-javalite:4.36.2")
    implementation(platform("androidx.compose:compose-bom:2026.06.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.10.0")
    implementation("androidx.webkit:webkit:1.12.1")
    implementation("androidx.documentfile:documentfile:1.0.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("com.squareup.okhttp3:okhttp:5.4.0")
    implementation("com.squareup.okhttp3:okhttp-sse:5.4.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
    implementation("org.tukaani:xz:1.10")
    implementation("org.commonmark:commonmark:0.30.1")
    implementation("org.commonmark:commonmark-ext-gfm-tables:0.30.1")
    implementation("org.commonmark:commonmark-ext-gfm-strikethrough:0.30.1")
    implementation("com.google.mlkit:text-recognition:16.0.1")
    implementation("com.google.mlkit:text-recognition-chinese:16.0.1")
    implementation("com.google.zxing:core:3.5.3")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testImplementation("com.squareup.okhttp3:mockwebserver:5.4.0")
    testImplementation(testFixtures(project(":mika-core")))

    androidTestImplementation(testFixtures(project(":mika-core")))
    androidTestImplementation(platform("androidx.compose:compose-bom:2026.06.00"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("com.squareup.okhttp3:mockwebserver:5.4.0")
    androidTestImplementation("junit:junit:4.13.2")
}

tasks.withType<Test>().configureEach {
    inputs.file(rootProject.file("../../licenses/THIRD_PARTY_NOTICES.md")).withPropertyName("thirdPartyNotices").optional()
}
