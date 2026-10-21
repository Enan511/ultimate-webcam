# Troubleshooting

## USB connection

Use a data-capable cable, unlock the phone, enable USB debugging, and approve the computer's fingerprint. Open UWebcam and allow camera access. With ADB from the extracted release:

```powershell
.\adb.exe devices -l
```

`unauthorized` requires approval on the phone. An empty list points to cable, debugging settings, or the phone's USB driver. Connect one phone while diagnosing. Close other tools using this app's sockets. Reinstall the bundled APK if needed and reopen the studio.

## Camera missing from Discord

Install the virtual camera from Setup and approve administrator permission. Fully quit Discord from its tray icon and reopen it to refresh the camera list. Look for **Ultimate Webcam** or **Unity Video Capture**. If registration failed, run `Install-Virtual-Camera.ps1` from the complete extracted package.

## Black picture or interrupted stream

Close other phone camera apps, check permission, reopen UWebcam, and use Start camera. Try a supported lower resolution or another lens. Task removal and desktop Quit intentionally stop capture; reopen the apps afterward. Camera modes vary by phone.

## Delay and orientation

Use Minimum delay and a direct USB connection for the fastest response. Full HD adds capture/encoder delay. Compare the desktop preview before diagnosing meeting-app buffering. A guaranteed 20 ms end-to-end limit is not provided; see [testing](TESTING.md).

Automatic orientation follows upright/sideways placement; flat placement retains the previous angle. Disable it for manual rotation. Meeting apps may mirror their local preview independently; inspect the received image or a recording before changing the studio mirror setting.

## Microphone

Install VB-Audio Virtual Cable separately, allow microphone permission, and enable studio audio. Choose CABLE Output in the meeting app; the studio writes to CABLE Input. Missing cable input leaves it muted.

## Diagnostics and reports

`studio.log` is beside the executable; use a writable folder. Remove serial numbers, local paths, snapshots, and private details before sharing reports. In [GitHub Issues](https://github.com/Enan511/ultimate-webcam/issues), include phone model, Android/Windows versions, lens, mode, and reproduction steps.
