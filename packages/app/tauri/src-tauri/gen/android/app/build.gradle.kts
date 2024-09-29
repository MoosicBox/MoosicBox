import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val keyProperties =
        Properties().apply {
            val propFile = rootProject.file("key.properties")
            if (propFile.exists()) {
                propFile.inputStream().use { load(it) }
            }
        }

val tauriProperties =
        Properties().apply {
            val propFile = file("tauri.properties")
            if (propFile.exists()) {
                propFile.inputStream().use { load(it) }
            }
        }

android {
    compileSdk = 34
    namespace = "com.moosicbox"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.moosicbox"
        minSdk = 24
        targetSdk = 34
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
        ndk { abiFilters += listOf("armeabi-v7a", "arm64a-v8a", "x86", "x86_64") }
    }
    signingConfigs {
        create("release") {
            keyAlias = keyProperties["keyAlias"] as String
            keyPassword = keyProperties["keyPassword"] as String
            storeFile = file(keyProperties["storeFile"] as String)
            storePassword = keyProperties["storePassword"] as String
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isMinifyEnabled = true
            proguardFiles(
                    *fileTree(".") { include("**/*.pro") }
                            .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                            .toList()
                            .toTypedArray()
            )
            signingConfig = signingConfigs.getByName("release")
        }
    }
    kotlinOptions { jvmTarget = "1.8" }
}

rust { rootDirRel = "../../../" }

dependencies {
    implementation("androidx.media3:media3-ui:1.4.0")
    implementation("androidx.media3:media3-session:1.4.0")
    implementation("androidx.media3:media3-exoplayer:1.4.0")
    implementation("androidx.webkit:webkit:1.6.1")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.8.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
