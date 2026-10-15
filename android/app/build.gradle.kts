import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("com.google.protobuf")
}

// Секреты не коммитим: берём из android/local.properties (gitignore) или env.
// В CI значения приходят из GitHub Secrets.
val localProps = Properties().apply {
    val f = rootProject.file("local.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

fun secret(key: String, default: String = ""): String =
    (localProps.getProperty(key) ?: System.getenv(key))?.takeIf { it.isNotBlank() } ?: default

// Release-подпись: включается, только если в окружении есть keystore
// (в CI — из GitHub Secrets, локально — из env). Иначе release без подписи.
val keystorePath: String? = System.getenv("KEYSTORE_FILE")?.takeIf { it.isNotBlank() }

// Версия приложения — из файла VERSION в корне репозитория.
val appVersion: String = rootProject.file("../VERSION")
    .takeIf { it.exists() }?.readText()?.trim()?.takeIf { it.isNotEmpty() } ?: "0.1"
val appVersionCode: Int = appVersion.split(".").let { p ->
    (p.getOrNull(0)?.toIntOrNull() ?: 1) * 10000 +
        (p.getOrNull(1)?.toIntOrNull() ?: 0) * 100 +
        (p.getOrNull(2)?.toIntOrNull() ?: 0)
}

android {
    namespace = "com.p2pchat.app"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.p2pchat.app"
        minSdk = 26
        targetSdk = 34
        versionCode = appVersionCode
        versionName = appVersion

        buildConfigField("String", "SIGNAL_URL", "\"${secret("SIGNAL_URL", "wss://90.188.89.63.sslip.io:9443")}\"")
        // Эндпоинт временных TURN-кред (TURN REST API).
        buildConfigField("String", "TURN_CREDS_URL", "\"${secret("TURN_CREDS_URL", "https://90.188.89.63.sslip.io:9443/turn-credentials")}\"")
    }

    // signaling-сообщения генерируются из src/main/proto (симлинк на общий proto/).

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KEY_ALIAS")
                keyPassword = System.getenv("KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (keystorePath != null) {
                signingConfig = signingConfigs.getByName("release")
            }
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.14"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }

    // Нативные библиотеки Rust (.so) кладём в jniLibs/ по ABI:
    //   app/src/main/jniLibs/arm64-v8a/libp2pchat_ffi.so
    //   app/src/main/jniLibs/x86_64/libp2pchat_ffi.so
    sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.4")
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.8.4")
    implementation("androidx.activity:activity-compose:1.9.1")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.4")

    implementation(platform("androidx.compose:compose-bom:2024.08.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")

    // UniFFI-биндинги используют JNA для загрузки нативной библиотеки.
    implementation("net.java.dev.jna:jna:5.13.0@aar")

    // WebRTC (libwebrtc для Android).
    implementation("io.github.webrtc-sdk:android:125.6422.07")

    // WebSocket для signaling.
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // protobuf-lite для сигнальных сообщений.
    implementation("com.google.protobuf:protobuf-javalite:3.25.3")

    // Генерация QR-кода ключа.
    implementation("com.google.zxing:core:3.5.3")

    debugImplementation("androidx.compose.ui:ui-tooling")
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:3.25.3"
    }
    generateProtoTasks {
        all().forEach { task ->
            task.builtins {
                create("java") {
                    option("lite")
                }
            }
        }
    }
}