import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

/**
 * Release signing, read from `keystore.properties` (written by tooling/setup-signing.ps1).
 * It is the key the retired Kotlin v1 shipped with: an update in place needs the
 * installed signature, or Android installs a second app beside it.
 *
 * Looked for at the repository root, then in the Gradle root. A git worktree does not
 * carry the gitignored file, so `-PkeystoreProperties=<absolute path>` points at it.
 * When absent the release APK is assembled unsigned and says so; it never falls back
 * to the debug signature.
 */
val keystorePropertiesFile = sequenceOf(
    providers.gradleProperty("keystoreProperties").orNull?.let { file(it) },
    rootProject.file("../../keystore.properties"),
    rootProject.file("keystore.properties"),
).firstOrNull { it?.isFile == true }

val keystoreProperties = Properties().apply {
    keystorePropertiesFile?.inputStream()?.use { load(it) }
}
val hasReleaseKey = keystoreProperties.getProperty("storeFile") != null

// Type of the alerts foreground service, set per build to measure it (RFC-PUSH-PROPIO §6):
// `-PalertsFgsType=connectedDevice` or `dataSync`. specialUse is the default.
val alertsFgsType = providers.gradleProperty("alertsFgsType").orNull ?: "specialUse"
val alertsFgsPermission = when (alertsFgsType) {
    "specialUse" -> "FOREGROUND_SERVICE_SPECIAL_USE"
    "connectedDevice" -> "FOREGROUND_SERVICE_CONNECTED_DEVICE"
    "dataSync" -> "FOREGROUND_SERVICE_DATA_SYNC"
    else -> error("alertsFgsType must be specialUse, connectedDevice or dataSync, not $alertsFgsType")
}
// connectedDevice starts only with one of the CHANGE_*_STATE permissions. The other types
// repeat ACCESS_NETWORK_STATE, which the network callback needs anyway.
val alertsFgsExtraPermission = if (alertsFgsType == "connectedDevice") "CHANGE_WIFI_STATE" else "ACCESS_NETWORK_STATE"

android {
    namespace = "dev.herdrnotify.herdr_notify"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "dev.herdrnotify.app"
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
        manifestPlaceholders["alertsFgsType"] = alertsFgsType
        manifestPlaceholders["alertsFgsPermission"] = alertsFgsPermission
        manifestPlaceholders["alertsFgsExtraPermission"] = alertsFgsExtraPermission
        buildConfigField("String", "ALERTS_FGS_TYPE", "\"$alertsFgsType\"")
    }

    buildFeatures {
        buildConfig = true
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = File(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
                enableV1Signing = true
                enableV2Signing = true
                enableV3Signing = true
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (hasReleaseKey) {
                signingConfigs.getByName("release")
            } else {
                logger.warn(
                    "No keystore.properties found: the release APK will be UNSIGNED. " +
                        "Run tooling/setup-signing.ps1 or pass -PkeystoreProperties=<path>.",
                )
                null
            }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

// Two tink artifacts carry the same classes and fail the debug dex merge: keep tink-android,
// which flutter_secure_storage brings.
configurations.all {
    exclude(group = "com.google.crypto.tink", module = "tink")
}

dependencies {
    // Read-only access to the Kotlin v1 pairing files for the migration offer.
    // Same versions as the Kotlin v1 app, which wrote those files.
    implementation("androidx.datastore:datastore-preferences:1.1.3")
    implementation("androidx.security:security-crypto:1.1.0-alpha06")

    // Watchdog every 15 minutes (Apache-2.0, free): restarts the alerts service if it stopped.
    implementation("androidx.work:work-runtime:2.10.0")

    testImplementation("junit:junit:4.13.2")
}

flutter {
    source = "../.."
}
