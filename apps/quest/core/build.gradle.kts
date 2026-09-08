plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}
kotlin { jvmToolchain(17) }
dependencies {
    implementation(libs.serialization.json)
    testImplementation(libs.junit)
}
sourceSets.test { resources.srcDir(rootProject.file("protocol")) }
tasks.test { useJUnit() }
