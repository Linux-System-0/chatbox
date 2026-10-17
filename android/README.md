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

On first launch the client asks for the backend as `host[:port]` plus an
`http`/`https` choice; `10.0.2.2:8080` is offered as a prefill hint, which
reaches the host machine from the Android emulator. Trust anchors, cleartext
hosts, and the "disable verification" switch are configured in the same flow.
See [`docs/android-network-security.md`](../docs/android-network-security.md)
for the rules and the threat model (ADR 0008). There is no debug/release split:
the same APK uses the runtime policy.

## Testing

```bash
cd android
./gradlew :app:testDebugUnitTest           # host matching, addresses, trust manager
./gradlew :app:connectedDebugAndroidTest   # MockWebServer + HandshakeCertificates
cd ..
node scripts/check-android-network-policy.mjs --require-apk
```

The last command is the CI guard: it rejects a direct `OkHttpClient`
construction, a compile-time network security config, the old
`BuildConfig.API_BASE_URL`, and an APK whose packaged manifest or resources
regress.

## Structure

```text
app/src/main/java/com/chatbox/android/
├── MainActivity.kt           # Compose entry point
├── data/                     # Models, Retrofit API, session storage
├── network/                  # Address parsing, host matching, trust policy, shared client
├── realtime/                 # Reconnecting WebSocket client
├── plugins/                  # Client plugins (language packs, themes)
└── ui/                       # Compose screens and view model
```

## Client plugins

Language packs and themes are **client plugins** (see `docs/plugin-system.md`):
they are installed and applied entirely on the device and never change server
behavior. `plugins/ClientPlugins.kt` provides the built-in English fallback
dictionary and the light/dark/system themes. Other languages, including
Simplified Chinese, come from language packs installed as plugin files.
