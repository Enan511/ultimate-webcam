# Installation and everyday use

## Download

Use the assets on the [latest release](https://github.com/Enan511/ultimate-webcam/releases/latest):

- `Ultimate-Webcam-1.3.2-Windows-x64.zip`: complete portable Windows package, including the phone APK, ADB, virtual camera DLLs, scripts, and documentation.
- `UWebcam-1.3.2.apk`: standalone Android installer for another phone.
- `SHA256SUMS.txt`: download hashes.

Extract the ZIP completely into a writable folder. Keep its folders together. Windows or Android may request permission to run an unsigned desktop executable or install the APK from your chosen file-transfer app. Obtain downloads from this repository's release page.

## Android setup

1. Install the APK on Android 9 or later and open **UWebcam**.
2. Allow camera access. Microphone permission is needed only for optional phone audio.
3. Enable Developer options by repeatedly tapping the build number in Android Settings, then enable **USB debugging**. Settings names vary by phone.
4. Connect a data-capable cable, unlock the phone, and approve the computer's debugging fingerprint.
5. Leave UWebcam open while starting the studio.

With one authorized phone connected, `Install-On-Phone.ps1` in the extracted Windows package can install/update the bundled APK. It does not enable debugging or grant camera permission for you.

## Windows and Discord setup

1. Open `Ultimate-Webcam.exe`. The authorized phone should connect automatically.
2. Open **Setup** and choose **Set up virtual camera** if needed. Approve Windows administrator permission. `Install-Virtual-Camera.ps1` performs the same registration.
3. Fully quit and reopen your meeting app after registration.
4. Select **Ultimate Webcam** or **Unity Video Capture** in its camera settings; use the name shown in the studio guide. In Discord this is Settings → Voice & Video → Camera.

The output is a virtual DirectShow camera. The receiving application must support virtual cameras. OBS is not required.

## Picture and modes

**Minimum delay** uses independent 240p JPEG frames, with a 480p fallback. **Fast 1080p** uses hardware H.264. Lenses and frame rates depend on the phone. Redmi A3 full-HD delivery is limited to 28 fps to reduce encoder queuing.

Choose the lens and camera controls in Camera and picture processing in Your look. The phone displays the processed broadcast preview at reduced resolution; virtual output retains the full output resolution.

Automatic orientation follows portrait, landscape, and upside-down placement. Laying the phone flat preserves its last stable angle. Disable it to rotate manually. Rotation by 90/270 degrees produces portrait output.

## Closing and stopping

**Setup → When I close the window** offers:

- **Quit and stop camera** (default): stop phone capture/audio and exit the studio.
- **Keep running in tray**: hide the window while broadcasting continues. Open it from the tray or choose tray **Quit** to stop capture and exit.

The choice is stored in `%APPDATA%\com.ultimatewebcam.studio\preferences.json`. Removing UWebcam from Android's recent apps stops its service/camera. Losing the desktop media connection pauses capture. Reopen the apps to start again.

## Optional microphone

Install [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) separately. Allow microphone access on the phone and enable audio in the studio. The studio writes to **CABLE Input**; choose **CABLE Output** as your meeting microphone. Audio is muted by default. Missing cable input does not cause phone audio to play through your speakers.

## Uninstall

Quit the studio. From the extracted package, unregister the virtual camera:

```powershell
.\Install-Virtual-Camera.ps1 -Uninstall
```

Remove UWebcam through Android Settings. After unregistering, the portable folder can be deleted. Optional saved Windows preferences are in `%APPDATA%\com.ultimatewebcam.studio`. Revoke the computer's USB debugging authorization on the phone when no longer needed.
