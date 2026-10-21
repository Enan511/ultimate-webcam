use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use window_vibrancy::apply_acrylic;

#[derive(Clone, serde::Serialize)]
pub struct UsbDeviceDetectedPayload {
    pub serial: String,
    pub model_name: String,
    pub authorized: bool,
}

pub struct AdbTrackerState {
    pub ignored_serials: Arc<RwLock<HashSet<String>>>,
    pub connected_devices: Arc<RwLock<HashSet<String>>>,
}

impl AdbTrackerState {
    pub fn new() -> Self {
        Self {
            ignored_serials: Arc::new(RwLock::new(HashSet::new())),
            connected_devices: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

pub fn start_adb_tracker(app: AppHandle, state: Arc<AdbTrackerState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            // Ensure ADB server is running
            let _ = crate::usb::adb::run(&["start-server"]).await;

            match connect_and_track(&app, &state).await {
                Ok(_) => {
                    log::info!("ADB tracker connection finished, reconnecting in 2s...");
                }
                Err(e) => {
                    log::warn!("ADB tracker connection error: {:?}, retrying in 3s...", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    });
}

async fn connect_and_track(app: &AppHandle, state: &Arc<AdbTrackerState>) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:5037").await?;

    // ADB client protocol: length prefix (4 hex digits) + command
    // "0012host:track-devices" (18 bytes = 0x0012)
    stream.write_all(b"0012host:track-devices").await?;

    // Read 4-byte status response ("OKAY" or "FAIL")
    let mut status = [0u8; 4];
    stream.read_exact(&mut status).await?;
    if &status != b"OKAY" {
        anyhow::bail!("ADB server responded with non-OKAY status: {:?}", status);
    }

    log::info!("Successfully subscribed to ADB host:track-devices event stream");

    // Continuously read device list updates:
    // 4 hex characters length + payload (lines of "<serial>\t<state>\n")
    let mut len_buf = [0u8; 4];

    loop {
        stream.read_exact(&mut len_buf).await?;
        let len_str = std::str::from_utf8(&len_buf)?;
        let payload_len = usize::from_str_radix(len_str, 16)?;

        let mut payload_buf = vec![0u8; payload_len];
        if payload_len > 0 {
            stream.read_exact(&mut payload_buf).await?;
        }

        let payload_text = String::from_utf8_lossy(&payload_buf);
        process_device_list_update(app, state, &payload_text).await?;
    }
}

async fn process_device_list_update(
    app: &AppHandle,
    state: &Arc<AdbTrackerState>,
    text: &str,
) -> anyhow::Result<()> {
    let mut current_serials = HashSet::new();

    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let serial = parts[0].to_string();
        let status = if parts.len() > 1 { parts[1] } else { "unknown" };
        if status == "device" {
            current_serials.insert(serial.clone());
        }

        let is_ignored = state.ignored_serials.read().await.contains(&serial);
        let was_already_connected = state.connected_devices.read().await.contains(&serial);

        if !is_ignored && !was_already_connected && (status == "device" || status == "unauthorized")
        {
            let authorized = status == "device";
            let model_name = query_device_model(&serial).await;

            log::info!(
                "New USB Device plugged in: {} ({}) [Authorized: {}]",
                model_name,
                serial,
                authorized
            );

            // Position & Show Prompt Window
            let payload = UsbDeviceDetectedPayload {
                serial: serial.clone(),
                model_name: model_name.clone(),
                authorized,
            };
            let _ = app.emit("usb-device-detected", payload);
            if !authorized {
                show_usb_prompt(app, &serial, &model_name, authorized);
            }
        }
    }

    // Check for unplugged devices: remove them from ignored list so re-plug prompts again
    let mut connected = state.connected_devices.write().await;
    let mut ignored = state.ignored_serials.write().await;

    let unplugged: Vec<String> = connected
        .iter()
        .filter(|s| !current_serials.contains(*s))
        .cloned()
        .collect();

    for serial in unplugged {
        log::info!(
            "Device {} physically unplugged, resetting session state",
            serial
        );
        connected.remove(&serial);
        ignored.remove(&serial);
        hide_usb_prompt(app);
        let app_state = app.state::<crate::AppState>();
        if app_state.pipeline.status.lock().unwrap().serial.as_ref() == Some(&serial) {
            app_state.pipeline.cancel();
            app_state
                .pipeline
                .wasapi
                .lock()
                .await
                .set_enabled(false)
                .ok();
            app_state.control.disconnect().await;
            app_state.pipeline.phase(app, "idle", "Phone disconnected");
        }
        let _ = app.emit("usb-device-disconnected", serial);
    }

    *connected = current_serials;
    Ok(())
}

pub async fn query_device_model(serial: &str) -> String {
    let output =
        crate::usb::adb::run(&["-s", serial, "shell", "getprop", "ro.product.model"]).await;

    match output {
        Ok(out) if out.status.success() => {
            let model = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !model.is_empty() {
                return model;
            }
        }
        _ => {}
    }
    format!("Android Device ({})", serial)
}

fn show_usb_prompt(app: &AppHandle, serial: &str, model_name: &str, authorized: bool) {
    if let Some(prompt_window) = app.get_webview_window("usb-prompt") {
        // Emit payload to prompt window React UI
        let _ = prompt_window.emit(
            "usb-device-detected",
            UsbDeviceDetectedPayload {
                serial: serial.to_string(),
                model_name: model_name.to_string(),
                authorized,
            },
        );

        // Position window at bottom-right of primary display work area
        if let Ok(Some(monitor)) = prompt_window.primary_monitor() {
            let screen_size = monitor.size();
            let scale_factor = monitor.scale_factor();
            let logical_w = (screen_size.width as f64) / scale_factor;
            let logical_h = (screen_size.height as f64) / scale_factor;

            let prompt_w = 420.0;
            let prompt_h = 210.0;
            let margin_right = 24.0;
            let margin_bottom = 64.0; // Above Windows 11 taskbar

            let x = logical_w - prompt_w - margin_right;
            let y = logical_h - prompt_h - margin_bottom;

            let _ = prompt_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }

        #[cfg(target_os = "windows")]
        let _ = apply_acrylic(&prompt_window, Some((18, 20, 30, 200)));

        let _ = prompt_window.show();
        let _ = prompt_window.set_focus();
    }
}

pub fn hide_usb_prompt(app: &AppHandle) {
    if let Some(prompt_window) = app.get_webview_window("usb-prompt") {
        let _ = prompt_window.hide();
    }
}
