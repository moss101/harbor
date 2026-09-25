import java.io.FileInputStream
import java.util.Properties

// scripts/package_android.sh writes android/key.properties from the
// operator's env vars. Nothing read it: the release build type asked for
// signingConfigs["debug"] unconditionally, so supplying an upload key
// produced a debug-signed bundle while the script reported "OPERATOR
// signing" — and Play rejects that on upload.
val keystorePropertiesFile = rootProject.file("key.properties")
val keystoreProperties = Properties().apply {
    if (keystorePropertiesFile.exists()) {
        FileInputStream(keystorePropertiesFile).use { load(it) }
    }
}
val hasOperatorKey = keystorePropertiesFile.exists()

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "dev.harbor.harbor_app"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.harbor.harbor_app"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName

        // NOTE: `ndk { abiFilters }` here does NOT restrict the packaged
        // ABIs — the Flutter Gradle plugin sets them from the build's
        // --target-platform and overwrites whatever is configured (dry
        // run 4 shipped armeabi-v7a and x86_64 with it in place). The
        // single supported ABI is selected at the build command instead:
        // `flutter build apk|appbundle --target-platform android-arm64`
        // (.github/workflows/release.yml).
    }

    // `--target-platform android-arm64` keeps Flutter's own libraries to
    // one ABI, but native libraries that arrive through dependency AARs
    // (libdartjni.so) are packaged for every ABI regardless, so the APK
    // still advertised lib/armeabi-v7a/ and lib/x86_64/ — enough for Play
    // to serve those devices a build with no Flutter runtime in it. The
    // release workflow asserts the packaged ABI list after the build.
    packaging {
        jniLibs {
            excludes += listOf(
                "lib/armeabi-v7a/**", "lib/armeabi/**",
                "lib/x86/**", "lib/x86_64/**",
            )
        }
    }

    signingConfigs {
        create("release") {
            if (hasOperatorKey) {
                storeFile = file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["storePassword"] as String
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
            }
        }
    }

    buildTypes {
        release {
            // The operator's upload key when android/key.properties is
            // present, the debug key otherwise so `flutter run --release`
            // still works. package_android.sh verifies which one actually
            // signed the output rather than trusting this.
            signingConfig = if (hasOperatorKey) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
