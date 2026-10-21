# USB Mobile Webcam System: Binary Wire & Control Protocol Specification

This document specifies the exact byte-level wire protocol and newline-delimited JSON control protocol used between the Android companion app and the Windows 11 desktop studio.

---

## 1. Port Allocation

| Port | Direction | Transport | Framing / Format | Purpose |
|---|---|---|---|---|
| **`27183`** | Android -> PC | TCP Socket (`adb forward tcp:27183 tcp:27183`) | 13-Byte Big-Endian Header + Binary Payload | High-throughput media streaming (H.264 video, PCM audio, capabilities manifest) |
| **`27185`** | PC -> Android | TCP Socket | Type 4 JPEG packets | Independent processed broadcast preview |
| **`27184`** | PC <-> Android | TCP Socket (`adb forward tcp:27184 tcp:27184`) | Newline-Delimited UTF-8 JSON (`\n`) | Low-latency bidirectional control channel (camera selection, zoom, exposure, torch, audio toggles) |

---

## 2. Media Stream Protocol (Port 27183)

### 2.1 Packet Header Layout (13 Bytes, Big-Endian)

Every packet emitted by the Android server across TCP port 27183 begins with a fixed 13-byte header followed by `payload_size` bytes of raw payload.

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Stream Type  |                                               |
+-+-+-+-+-+-+-+-+                                               +
|                                                               |
+                       Presentation Timestamp                  +
|                              (pts_us)                         |
+                                               +-+-+-+-+-+-+-+-+
|                                               |               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+               +
|                          Payload Size                         |
+                               +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
|                        Payload Bytes                          |
|                       (N = payload_size)                      |
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

| Byte Offset | Field Name | Type | Endianness | Description |
|---|---|---|---|---|
| `0` | `stream_type` | `uint8` | - | `0x01` = H.264 Video, `0x02` = PCM Audio, `0x03` = Capabilities Manifest |
| `1 .. 8` | `pts_us` | `int64` | Big-Endian | Capture timestamp normalized to Android elapsed-realtime microseconds |
| `9 .. 12` | `payload_size` | `uint32` | Big-Endian | Size of the subsequent payload in bytes (`N`) |
| `13 .. (13 + N - 1)` | `payload` | `[uint8; N]` | - | Raw payload bytes corresponding to `stream_type` |

### 2.2 Stream Type Definitions

#### `0x01`: H.264 / AVC Annex-B Video Frame
- Payload contains one or more H.264 Annex-B Network Abstraction Layer Units (NALUs) starting with standard 4-byte or 3-byte start codes (`00 00 00 01` or `00 00 01`).
- **Keyframe Invariant**: When `MediaCodec.BUFFER_FLAG_KEY_FRAME` is emitted, the Android encoder **MUST** prepend the cached Sequence Parameter Set (SPS) and Picture Parameter Set (PPS) NAL units before the IDR slice payload. This ensures the Rust decoder (OpenH264) can synchronize and render immediately without waiting for an out-of-band parameter update.

#### `0x02`: Raw PCM Audio Chunk
- Carries uncompressed linear PCM audio captured by Android's `AudioRecord`.
- Sample format: **48,000 Hz**, **16-bit Signed Integer**, **Mono (1 Channel)**.
- Endianness of audio samples: **Little-Endian** (standard Android/Windows PCM convention).
- Default packet size: 960 bytes (representing exactly 480 samples = 10 milliseconds of audio).
- Audio is **disabled by default** on both Android and PC to prevent background resource consumption.

#### `0x03`: Device Capabilities Manifest
- Emitted immediately upon TCP socket handshake on Port 27183.
- Payload is a valid UTF-8 JSON document detailing the phone model and hardware camera/encoder capabilities.

```json
{
  "type": "CAPABILITIES",
  "model": "Nothing Phone (3a)",
  "manufacturer": "Nothing",
  "soc": "MediaTek Dimensity 7200 Pro",
  "cameras": [
    {
      "id": "0",
      "name": "Rear Main Wide (50MP ISOCELL GN9)",
      "lensFacing": "BACK",
      "focalLength": 24.0,
      "aperture": 1.88,
      "hasFlash": true,
      "opticalStabilization": true,
      "physicalIds": ["0"],
      "modes": [
        { "width": 3840, "height": 2160, "fps": 30 },
        { "width": 1920, "height": 1080, "fps": 60 },
        { "width": 1920, "height": 1080, "fps": 30 },
        { "width": 1280, "height": 720, "fps": 60 }
      ],
      "zoomRange": { "min": 1.0, "max": 10.0 }
    },
    {
      "id": "2",
      "name": "Rear 2x Telephoto (50MP)",
      "lensFacing": "BACK",
      "focalLength": 50.0,
      "aperture": 2.0,
      "hasFlash": true,
      "opticalStabilization": false,
      "physicalIds": ["2"],
      "modes": [
        { "width": 1920, "height": 1080, "fps": 60 },
        { "width": 1920, "height": 1080, "fps": 30 }
      ],
      "zoomRange": { "min": 1.0, "max": 8.0 }
    },
    {
      "id": "1",
      "name": "Front Selfie (32MP)",
      "lensFacing": "FRONT",
      "focalLength": 22.0,
      "aperture": 2.2,
      "hasFlash": false,
      "opticalStabilization": false,
      "physicalIds": ["1"],
      "modes": [
        { "width": 1920, "height": 1080, "fps": 60 },
        { "width": 1920, "height": 1080, "fps": 30 },
        { "width": 1280, "height": 720, "fps": 30 }
      ],
      "zoomRange": { "min": 1.0, "max": 4.0 }
    }
  ],
  "audioSupported": true,
  "maxSupportedBitrate": 50000000
}
```

---

## 3. Bidirectional Control Protocol (Port 27184)

Messages are single-line JSON strings terminated by a newline character (`\n`).

### 3.1 Client Commands (PC -> Android)

#### 1. Configure Camera Stream
Requests the companion app to configure the camera pipeline and hardware encoder:
```json
{
  "cmd": "CONFIGURE",
  "cameraId": "0",
  "width": 3840,
  "height": 2160,
  "fps": 30,
  "bitrate": 35000000,
  "codec": "h264"
}
```

#### 2. Toggle Audio Stream
Enables or disables the microphone capture stream over Port 27183:
```json
{
  "cmd": "SET_AUDIO",
  "enabled": true
}
```

#### 3. Toggle Rear Torch
Turns the rear camera LED flash on or off:
```json
{
  "cmd": "SET_TORCH",
  "enabled": true
}
```

#### 4. Set Hardware Zoom Ratio
Adjusts `CONTROL_ZOOM_RATIO`:
```json
{
  "cmd": "SET_ZOOM",
  "ratio": 2.0
}
```

#### 5. Set Exposure Compensation
Adjusts exposure compensation indices within device limits:
```json
{
  "cmd": "SET_EXPOSURE",
  "compensation": 2
}
```

#### 6. Stop Stream
Stops encoding and releases camera hardware gracefully:
```json
{
  "cmd": "STOP_STREAM"
}
```

### 3.2 Server Responses (Android -> PC)

#### 1. Command Acknowledgment
```json
{
  "status": "OK",
  "cmd": "CONFIGURE",
  "activeConfig": {
    "cameraId": "0",
    "width": 3840,
    "height": 2160,
    "fps": 30,
    "bitrate": 35000000
  }
}
```

#### 2. Error Response
```json
{
  "status": "ERROR",
  "cmd": "CONFIGURE",
  "error": "Hardware codec cannot allocate 4K surface; falling back to 1080p@30",
  "fallbackConfig": {
    "cameraId": "0",
    "width": 1920,
    "height": 1080,
    "fps": 30
  }
}
```


## Version 1.1 control additions

GET_STATE returns status OK and a telemetry object with the active camera configuration, streaming/client state, audio, torch, zoom, exposure, and any camera error. START_STREAM resumes a paused camera. Every control request has exactly one newline-delimited response. Rejected commands return status ERROR and an error string; the connection remains usable. Existing media header and stream type values are unchanged.


## 1.1 processed preview and latency timing

Media remains a 13-byte big-endian header: type u8, timestamp u64 microseconds, payload length u32, then payload bytes.

- Phone to PC type 1 video timestamps are normalized to Android elapsed realtime (boot time).
- GET_STATE replies add clockUs in that same timebase, sampled immediately before replying. The PC uses the lowest-latency round trips to estimate clock offset and uncertainty.
- Phone to PC type 6 carries one byte: 1 when the visible companion wants the studio preview, 0 when hidden or dimmed. The phone repeats this subscription on media connection.
- PC to phone type 4 carries a JPEG of the processed output at reduced resolution and up to 30 fps in 1.3. Timestamp is currently zero. The phone limits payloads to 512 KiB. No extra Camera2 output target is needed.
- Preview encoding and writes run separately from the video receiver. The desktop image slot always replaces an older completed frame. Video reference frames remain decoded even when image conversion is skipped to recover from a stall.


## 1.2 independent frames and connection recovery

- The 13-byte header remains unchanged on media port 27183.
- Type 8 is an independent JPEG camera frame; timestamp uses Android boot time as with H264.
- Type 7 is a supported diagnostic I420 frame: big-endian width u32 and height u32 followed by Y, U, V bytes. The user-facing minimum-delay profile uses type 8 because it measured faster on this USB connection.
- Processed phone preview type 4 now uses a separate loopback TCP connection on port 27185. Preview subscription type 6 still travels phone-to-PC on the media connection. A preview failure cannot close the video connection.
- SET_PROFILE accepts minimum (320x240 independent frames, 640x480 fallback) and 1080 (1920x1080 hardware H264). CONFIGURE can explicitly name transport jpeg, h264, or diagnostic raw.
- REQUEST_KEYFRAME is an acknowledged control command. The PC uses it when the bounded encoded H264 queue overflows; it drops the dependent chain and resumes from a new IDR.
- GET_STATE telemetry includes transport. Capture recovery retains the selected camera and mode.

The PC negotiates transport flow control by writing a type 10 header with zero payload on the media socket. It acknowledges each received video frame with type 9, echoing that frame’s timestamp and zero payload. The phone permits one unacknowledged video frame; if a dependent H264 frame must be skipped, it requests an IDR after acknowledgement. Legacy receivers that do not negotiate retain the bounded-queue path.

## Automatic orientation (1.3)

Media type 11 (0x0B) carries UTF-8 JSON: {"rotation":90}. Only 0, 90, 180, and 270 clockwise degrees are valid. It is sent at connection, lens/configuration change, and stable device-orientation change. The receiver associates the current rotation with each incoming video frame before enqueueing it, so queued decoding cannot apply a new angle to older frames. This automatic angle is added to the user’s manual rotation. The native output, desktop preview, and returned phone preview all use the same result.

SET_AUTO_ORIENTATION accepts {"cmd":"SET_AUTO_ORIENTATION","enabled":true}. The setting is retained on the phone. GET_STATE telemetry includes autoOrientation and rotationDegrees. Sensor orientation is included per lens in the capabilities manifest. Orientation sensing runs in the foreground service and does not require Android’s screen auto-rotate setting.
