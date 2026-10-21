# Third-party components

Upstream licenses remain applicable. Release packages retain UnityCapture's license and include collected dependency notices under `third-party-licenses/`.

| Component | Use / notice |
| --- | --- |
| UnityCapture | DirectShow output; [upstream commit](https://github.com/schellingb/UnityCapture/tree/3ed54c325e0ad71afcf4f246c07e5e17b3d7f2d2) and [retained MIT license](drivers/UnityCapture/LICENSE) |
| Android Debug Bridge | USB forwarding/install; Android SDK Platform Tools/AOSP notice included with the release |
| OpenH264 and bindings | H.264 decode; locked openh264/openh264-sys2 package/upstream notices collected with the release |
| Tauri, wgpu, Tokio, image, CPAL, Windows bindings, other Rust dependencies | Desktop runtime; Cargo.lock and collected package notices |
| React, Framer Motion, Lucide, other frontend dependencies | Desktop interface; package-lock.json and collected package notices |
| AndroidX, Compose, Kotlin, kotlinx.coroutines | Android runtime; Gradle version catalog and upstream Apache notices included in release |

`scripts/collect-notices.mjs` gathers available installed Rust/frontend license files without copying private local paths. The Rust inventory also includes build dependencies. Packages absent from the builder's cache may need separate notices. Review notices whenever dependencies or distribution contents change. VB-Audio Virtual Cable is a separate download and is not bundled.
