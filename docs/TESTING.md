# Testing and known limits

## Release 1.3.2 verification

Frontend, Windows native release, and Android release builds passed. Five native unit tests passed, covering orientation, optimized color conversion, control/socket behavior, and STOP_STREAM before disconnect.

Version 1.3.2 / code 7 was installed and verified on Redmi A3 and Nothing Phone (3a). Both stream modes were physically exercised on both models across this work; the last Redmi action was installation/version verification, not a new benchmark.

Nothing Phone (3a) front camera, 15-second functional checks:

| Mode | Frames | Approx. fps | Decode errors | Stalls |
| --- | ---: | ---: | ---: | ---: |
| Minimum delay | 456 | 30.1 | 0 | 0 |
| Fast 1080p | 465 | 29.9 | 0 | 0 |

These ran alongside a native build. They establish short functional operation, not long-duration stability or controlled end-to-end delay.

On Nothing Phone (3a), removing the app from recent apps stopped its service/camera; media disconnection paused camera/audio; Windows close-to-tray kept capture running; Windows Quit exited the process and released phone camera/audio. The close preference was read back from disk. The supplied Windows icon was verified in the title bar/interface, and Android launcher variants were generated and inspected.

## Delay and limitations

Redmi A3 version 1.3 checks measured about 80–83 ms median in Minimum delay at 240p/30 fps and 145–175 ms at 1080p/about 28 fps. Front runs lasted 60 seconds per mode and rear runs 20 seconds, with no stalls/decode errors. Its hardware encoder reported three frames of delay even when zero was requested.

Short Nothing runs produced roughly 60 ms and 111 ms median timestamp estimates. Concurrent build load and short duration prevent treating them as controlled comparisons.

**The 20 ms target is unmet.** Estimates compare camera timestamps with calibrated PC time; they exclude displays, meeting-app buffering, and uncertainty in the phone sensor clock. Optical end-to-end tests and longer quiescent runs remain useful. Support on every phone, mode, and receiving app is not established.

## Native tests

```powershell
cd pc-webcam-studio
npm run build
cargo test --locked --release --lib --features custom-protocol --manifest-path src-tauri/Cargo.toml
```

## Device diagnostics

Stop the studio, open UWebcam on one authorized phone, and forward sockets using ADB from the extracted release:

```powershell
.\adb.exe forward tcp:27183 tcp:27183
.\adb.exe forward tcp:27184 tcp:27184
.\adb.exe forward tcp:27185 tcp:27185
```

From the source root, after building the frontend:

```powershell
cargo run --locked --release --features custom-protocol --manifest-path pc-webcam-studio/src-tauri/Cargo.toml --bin stream-check -- --camera 1 --profile minimum --seconds 60
cargo run --locked --release --features custom-protocol --manifest-path pc-webcam-studio/src-tauri/Cargo.toml --bin stream-check -- --camera 1 --profile 1080 --seconds 60
```

Camera IDs vary; choose a supported ID from that phone's manifest. `--filtered` exercises monochrome portrait processing. Reports and snapshots go under `artifacts`; review their privacy before sharing.

`gpu-check` verifies identity pixels, mirroring, and portrait rotation. `conversion-check` needs an H.264 keyframe fixture. `tests/phone-controls.ps1` sends real phone controls; `tests/latency-check.cjs` measures encoded packet arrival, not displayed output. Diagnostics change phone state and require exclusive socket access.
