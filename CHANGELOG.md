# Changelog

## 1.3.2 — 2026-10-21

- Apply the supplied Ultimate Webcam icon on Windows/Android, including adaptive/themed Android launchers and notification branding.
- Stop Android camera service on task removal; pause camera/audio on media disconnection.
- Add saved Windows close preference: Quit and stop camera, or Keep running in tray.
- Centralize Windows shutdown with a fresh, bounded STOP_STREAM connection.
- Install/verify on Nothing Phone (3a) and Redmi A3.
- Publish source documentation, a complete Windows release, and standalone Android APK.

## Earlier work

- Minimum delay and Fast 1080p modes, bounded queues, optimized conversion, and frame-driven previews.
- Processed broadcast preview on the phone, automatic orientation, camera controls, and filters.
- Windows virtual camera integration and hidden ADB subprocesses.

A 20 ms end-to-end delay guarantee is not achieved. See [testing](docs/TESTING.md).
