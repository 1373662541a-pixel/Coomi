# Coomi Android

Coomi Android embeds the complete Rust implementation from `apps/coomi-rs` in a
Termux-based Android shell. The Vue application in `apps/web` talks directly to
the Rust HTTP and WebSocket server. There is no Python Coomi runtime or bridge.

## Runtime layout

- `apps/coomi-rs`: Rust workspace and the `coomi` CLI/server.
- `apps/web`: Vue frontend served by `coomi serve` on loopback.
- `apps/coomi-app`: Android application and Termux bootstrap.
- `app/src/main/jniLibs/arm64-v8a/libcoomi.so`: staged ARM64 PIE executable.
- `app/src/main/assets/web.zip`: staged production frontend.

The installed app creates `$PREFIX/bin/coomi` as a symlink to the executable in
the APK native library directory. The Android UI and Termux CLI share
`$HOME/.coomi/config/providers.json` and the same engine process.

## Build

Prerequisites: JDK 17, Android SDK/NDK, Rust, Node.js, and npm.

```powershell
.\gradlew.bat :app:assembleDebug
```

The Gradle build installs the `aarch64-linux-android` Rust target, builds the
Vue frontend, cross-compiles `coomi`, stages both payloads, and creates:

```text
apps/coomi-app/app/build/outputs/apk/debug/coomi-app_apt-android-7-debug_arm64-v8a.apk
```

## Deploy

```powershell
adb install -r -t apps/coomi-app/app/build/outputs/apk/debug/coomi-app_apt-android-7-debug_arm64-v8a.apk
adb shell am start -n com.termux/app.coomi.CoomiLauncherActivity
```

The application ID remains `com.termux` because the bundled bootstrap contains
paths compiled for that package name.
