import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "com.pmgr.password_manager"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.pmgr.password_manager"
        minSdk = 28
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        create("release") {
            // 发布签名：环境变量注入，keystore 不进 git
            System.getenv("PMGR_KEYSTORE_PATH")?.let {
                // 部分配置时尽早报出明确的中文错误，而不是 packageRelease 阶段的隐晦失败
                require(!System.getenv("PMGR_KEYSTORE_PASSWORD").isNullOrEmpty()) { "PMGR_KEYSTORE_PASSWORD 未设置" }
                require(!System.getenv("PMGR_KEY_ALIAS").isNullOrEmpty()) { "PMGR_KEY_ALIAS 未设置" }
                require(!System.getenv("PMGR_KEY_PASSWORD").isNullOrEmpty()) { "PMGR_KEY_PASSWORD 未设置" }
                storeFile = file(it)
                storePassword = System.getenv("PMGR_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("PMGR_KEY_ALIAS")
                keyPassword = System.getenv("PMGR_KEY_PASSWORD")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
            // 未配置 PMGR_KEYSTORE_PATH 时回落 debug 签名（仅本地调试）
            signingConfig = if (System.getenv("PMGR_KEYSTORE_PATH") != null)
                signingConfigs.getByName("release") else signingConfigs.getByName("debug").also {
                    println("警告：未配置 PMGR_KEYSTORE_PATH，本次 release 回落 debug 签名，仅限本地调试，请勿分发")
                }
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")