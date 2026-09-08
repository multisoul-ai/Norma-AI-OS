plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "ai.multisoul.norma.quest"
    compileSdk = 34
    defaultConfig {
        applicationId = "ai.multisoul.norma.quest"
        minSdk = 34
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += "arm64-v8a" }
    }
    buildFeatures { compose = true; buildConfig = true }
    buildTypes {
        debug { applicationIdSuffix = ".debug" }
        release { isMinifyEnabled = false }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    packaging { resources.excludes += "/META-INF/{AL2.0,LGPL2.1,LICENSE}" }
}
kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }

dependencies {
    implementation(project(":core"))
    implementation(libs.spatial.base)
    implementation(libs.spatial.toolkit)
    implementation(libs.spatial.vr)
    implementation(libs.spatial.compose)
    implementation(libs.spatial.isdk)
    implementation(libs.spatial.uiset)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material)
    implementation(libs.compose.preview)
    implementation(libs.activity.compose)
    // Match Meta StarterSample and keep lint/test classpaths on the published AndroidX core.
    implementation(libs.core.ktx)
    implementation(libs.okhttp)
    debugImplementation(libs.compose.tooling)
}
