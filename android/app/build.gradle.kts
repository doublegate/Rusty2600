plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.doublegate.rusty2600"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.doublegate.rusty2600"
        // 26 (Oreo): the oldest API level with a non-deprecated AudioTrack
        // float-PCM path (`AudioFormat.ENCODING_PCM_FLOAT`), matching the
        // `f32` samples `MobileEmulator::run_frame` already returns.
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.11.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    // `librusty2600_mobile.so` per ABI lives in `src/main/jniLibs/<abi>/` —
    // pre-built by `cargo ndk` (see `../regenerate-bindings.sh`), not by a
    // Gradle-driven Cargo build. No `externalNativeBuild`/CMake needed since
    // this crate has no C/C++ sources of its own.
}

dependencies {
    // The UniFFI-generated Kotlin bindings (`uniffi/rusty2600_mobile/
    // rusty2600_mobile.kt`, copied in verbatim by `regenerate-bindings.sh`)
    // call into the native library via JNA, not raw `System.loadLibrary` +
    // hand-written JNI signatures — this is what lets `rusty2600-mobile`
    // ship with zero hand-written `unsafe`/JNI glue on either side.
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
}
