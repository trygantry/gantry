plugins {
    id("com.android.application")
}

android {
    namespace = "dev.trygantry.fixture"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.trygantry.fixture"
        minSdk = 24
        targetSdk = 35
        versionCode = 42
        versionName = "1.2.3"
    }

    buildTypes {
        getByName("release") {
            isDebuggable = false
            signingConfig = signingConfigs.getByName("release")
        }
    }
}
