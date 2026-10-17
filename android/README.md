# ChatBox Android Client

A native Kotlin + Jetpack Compose client for ChatBox. It talks to the backend
over the same REST control plane (`docs/api.md`) and WebSocket data plane
(`docs/realtime-protocol.md`) as the web client.

## Build

```bash
cd android
./gradlew assembleDebug
```

The debug APK is written to `app/build/outputs/apk/debug/app-debug.apk`. A local
JDK 17 and the Android SDK are required; set `ANDROID_HOME` or add a
`local.properties` with `sdk.dir=/path/to/android-sdk`.

The backend base URL is set in `app/build.gradle.kts` as
`BuildConfig.API_BASE_URL`. The default `http://10.0.2.2:8080` reaches the host
machine from the Android emulator.

## Structure

```text
app/src/main/java/com/chatbox/android/
├── MainActivity.kt           # Compose entry point
├── data/                     # Models, Retrofit API, session storage
├── realtime/                 # Reconnecting WebSocket client
├── plugins/                  # Client plugins (language packs, themes)
└── ui/                       # Compose screens and view model
```

## Client plugins

Language packs and themes are **client plugins** (see `docs/plugin-system.md`):
they are installed and applied entirely on the device and never change server
behavior. `plugins/ClientPlugins.kt` provides the built-in English and
Simplified Chinese language packs and the light/dark/system themes.
