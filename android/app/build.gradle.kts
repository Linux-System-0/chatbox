plugins {
	id("com.android.application")
	id("org.jetbrains.kotlin.android")
	id("org.jetbrains.kotlin.plugin.compose")
	id("org.jetbrains.kotlin.plugin.serialization")
}

android {
	namespace = "com.chatbox.android"
	compileSdk = 35

	defaultConfig {
		applicationId = "com.chatbox.android"
		minSdk = 26
		targetSdk = 35
		versionCode = 1
		versionName = "0.1.0"
		testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
	}

	buildFeatures {
		compose = true
	}

	compileOptions {
		sourceCompatibility = JavaVersion.VERSION_17
		targetCompatibility = JavaVersion.VERSION_17
	}

	kotlinOptions {
		jvmTarget = "17"
	}
}

dependencies {
	implementation(platform("androidx.compose:compose-bom:2024.10.01"))
	implementation("androidx.compose.ui:ui")
	implementation("androidx.compose.ui:ui-tooling-preview")
	implementation("androidx.compose.material3:material3")
	implementation("androidx.activity:activity-compose:1.9.3")
	implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
	implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
	implementation("androidx.core:core-ktx:1.13.1")

	implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
	implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

	implementation("com.squareup.okhttp3:okhttp:4.12.0")
	implementation("com.squareup.retrofit2:retrofit:2.11.0")
	implementation("com.jakewharton.retrofit:retrofit2-kotlinx-serialization-converter:1.0.0")

	debugImplementation("androidx.compose.ui:ui-tooling")

	testImplementation("junit:junit:4.13.2")
	testImplementation("com.squareup.okhttp3:okhttp-tls:4.12.0")

	androidTestImplementation("androidx.test:core:1.6.1")
	androidTestImplementation("androidx.test:runner:1.6.2")
	androidTestImplementation("androidx.test.ext:junit:1.2.1")
	androidTestImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
	androidTestImplementation("com.squareup.okhttp3:okhttp-tls:4.12.0")
	androidTestImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
}
