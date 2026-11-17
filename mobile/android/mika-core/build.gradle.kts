import com.google.protobuf.gradle.*

plugins {
    id("com.google.protobuf")
    id("org.jetbrains.kotlin.jvm")
    id("org.jetbrains.kotlin.plugin.serialization")
    `java-test-fixtures`
}

kotlin {
    jvmToolchain(17)
}

val protobufVersion = "4.36.2"

val traceSchemas by configurations.creating { isTransitive = false }
val extractTraceSchemas by tasks.registering(Sync::class) {
    from({ zipTree(traceSchemas.singleFile) }) {
        include(
            "opentelemetry/proto/common/v1/common.proto",
            "opentelemetry/proto/resource/v1/resource.proto",
            "opentelemetry/proto/trace/v1/trace.proto",
            "opentelemetry/proto/collector/trace/v1/trace_service.proto",
        )
    }
    into(layout.buildDirectory.dir("trace-schemas"))
}
sourceSets.main { proto.srcDir(extractTraceSchemas) }

protobuf {
    protoc { artifact = "com.google.protobuf:protoc:$protobufVersion" }
    generateProtoTasks {
        all().configureEach {
            builtins {
                named("java") { option("lite") }
            }
        }
    }
}

dependencies {
    implementation("com.google.protobuf:protobuf-javalite:$protobufVersion")
    traceSchemas("io.opentelemetry.proto:opentelemetry-proto:1.11.1-alpha")
    implementation("com.squareup.okhttp3:okhttp:5.4.0")
    implementation("com.squareup.okhttp3:okhttp-sse:5.4.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    implementation("org.apache.commons:commons-compress:1.28.0")
    implementation("org.tukaani:xz:1.10")

    testImplementation("junit:junit:4.13.2")
    testImplementation("com.squareup.okhttp3:mockwebserver:5.4.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.10.2")
    testImplementation("org.conscrypt:conscrypt-openjdk-uber:2.7.0")
    testFixturesImplementation("junit:junit:4.13.2")
}

val knowledgeIndex by tasks.registering(JavaExec::class) {
    val resources = file("src/main/resources")
    val out = layout.buildDirectory.dir("generated/knowledge-index")
    inputs.dir(resources.resolve("knowledge")).withPropertyName("knowledge")
    inputs.dir(resources.resolve("skills")).withPropertyName("skills")
    outputs.dir(out)
    classpath = sourceSets.main.get().output.classesDirs + configurations.runtimeClasspath.get()
    mainClass.set("com.wangzan101.mika.core.knowledge.KnowledgeIndex")
    argumentProviders.add(CommandLineArgumentProvider {
        listOf(resources.absolutePath, out.get().file("knowledge/index.json").asFile.absolutePath)
    })
}
sourceSets.main { resources.srcDir(knowledgeIndex) }

tasks.test {
    exclude("**/RemoteCliContractTest*")
    exclude("**/RelayGatewayIntegrationTest*")
    classpath = sourceSets.main.get().output + classpath
    inputs.dir(rootProject.file("mika-agent/src")).withPropertyName("mikaAgentSources").optional()
    systemProperty("mika.snapshotDir", file("src/test/resources/loop-snapshots").absolutePath)
    systemProperty("mika.promptSurfaceDir", file("src/test/resources/prompt-surface").absolutePath)
    val updateSnapshots = project.hasProperty("updateSnapshots")
    inputs.property("updateSnapshots", updateSnapshots)
    if (updateSnapshots) {
        systemProperty("mika.updateSnapshots", "true")
        outputs.upToDateWhen { false }
    }
}
