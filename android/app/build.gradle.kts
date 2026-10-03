plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// versionCode grows with every CI build (the PC offers the phone an update when it is higher).
val base = "0.4.0"
val build = System.getenv("GITHUB_RUN_NUMBER")?.toIntOrNull() ?: 1

android {
    namespace = "com.nikgob.clockmanage"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.nikgob.clockmanage"
        minSdk = 26
        targetSdk = 35
        versionCode = build
        versionName = "$base ($build)"
    }

    // One fixed key for every build, so a new APK installs over the old one (pairing and
    // settings survive). It only signs this sideloaded app; keep the APK to yourself.
    signingConfigs {
        create("fixed") {
            storeFile = file("clockmanage.keystore")
            storePassword = "clockmanage"
            keyAlias = "clockmanage"
            keyPassword = "clockmanage"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("fixed")
        }
        debug {
            signingConfig = signingConfigs.getByName("fixed")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        buildConfig = true
    }
    lint {
        // Sideloaded app: Play-only policy checks (exact alarms etc.) must not fail the build.
        abortOnError = false
        checkReleaseBuilds = false
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}
