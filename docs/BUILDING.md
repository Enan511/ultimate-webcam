# Build from source

## Requirements

- Windows x64 and a DirectX 12-capable GPU for the desktop runtime.
- Node.js/npm; Node 24 was used for the published build.
- Rust and Windows native C/C++ build tools. The published build used Rust 1.99.1 with `x86_64-pc-windows-gnu` and installed GNU tools. MSVC needs the Visual Studio C++ tools/Windows SDK and has not been validated here.
- JDK 17, Android SDK platform 35, and SDK build tools. The included wrapper uses Gradle 8.9; Android Gradle Plugin is 8.6.0 and Kotlin is 2.0.0.
- Network access for first-time dependency downloads. Offline builds require populated caches.

## Windows

```powershell
git clone https://github.com/Enan511/ultimate-webcam.git
cd ultimate-webcam\pc-webcam-studio
npm ci
npm run build:desktop
```

The Tauri command builds the frontend and native executable without an installer bundle. If GNU resource compilation has trouble with spaces in a path, set a target directory without spaces, for example `$env:CARGO_TARGET_DIR='C:\cargo-target\ultimate-webcam'`.

Alternatively, build explicitly from `pc-webcam-studio`:

```powershell
npm run build
cargo build --locked --release --features custom-protocol --bins --manifest-path src-tauri/Cargo.toml
```

The main output is `src-tauri/target/release/pc-webcam-studio.exe`, or `$env:CARGO_TARGET_DIR\release\pc-webcam-studio.exe` when overridden. Copy/rename it to `Ultimate-Webcam.exe` for distribution. The frontend `dist` folder must exist before the native `custom-protocol` build; its assets are embedded.

For development, use `npm run tauri dev`. Browser-only `npm run dev` lacks native USB and virtual-camera commands.

## Android

Set `JAVA_HOME` to JDK 17. Configure the Android SDK through Android Studio, `ANDROID_HOME`, or private `android-webcam-app/local.properties` with `sdk.dir=...`.

```powershell
cd android-webcam-app
.\gradlew.bat assembleRelease
```

Output: `app/build/outputs/apk/release/app-release.apk`. Copy it to the distributable as `Phone App/UWebcam.apk`.

The current release build uses Gradle's local **debug signing configuration**, with R8/resource shrinking enabled. The original local key is excluded from Git. A fresh checkout uses the builder's own key; its APK usually cannot update an installation signed by the original key. Uninstall that app first (removing settings), or privately use the same signing key. Configure private release signing before maintaining a long-term signed distribution. Never publish keystores or passwords.

## Distribution layout

Compiled apps, ADB, and driver DLLs are release assets, not source files. Use this layout:

```text
Ultimate-Webcam.exe
adb.exe, AdbWinApi.dll, AdbWinUsbApi.dll
Install-On-Phone.ps1, Install-Virtual-Camera.ps1
Phone App/UWebcam.apk, Phone App/README.txt
drivers/UnityCapture/UnityCaptureFilter32.dll
drivers/UnityCapture/UnityCaptureFilter64.dll
drivers/UnityCapture/LICENSE, drivers/UnityCapture/source.txt
README.md, docs/, LICENSE, THIRD_PARTY_NOTICES.md, third-party-licenses/
```

Obtain ADB from Android SDK Platform Tools and UnityCapture from the referenced upstream project or this project's release. The installer verifies exact DLL hashes; replacing/rebuilding drivers requires reviewing and updating those hashes. Companion files must be next to the studio executable.

Keep versions aligned across npm manifests/lockfile, Rust manifest/lockfile, Tauri config, Android versionName/versionCode, and the desktop APK upgrade threshold in `lib.rs`.

## Tests and notices

See [TESTING.md](TESTING.md). With dependencies installed, run `node scripts/collect-notices.mjs` from the source root to collect available Rust/frontend licenses into `third-party-licenses`. Ship them with the release, UnityCapture's license, and the matching ADB notice. Review notices when dependencies change.
