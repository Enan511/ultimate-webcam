pub mod gpu;
pub mod sinks;
pub mod stream;
pub mod usb;
use crate::gpu::wgpu_pipeline::FilterUniforms;
use crate::stream::control_client::ControlClient;
use crate::stream::tcp_receiver::{start_stream_receiver, StreamPipeline, StreamStatus};
use crate::usb::{
    adb,
    adb_tracker::{start_adb_tracker, AdbTrackerState, UsbDeviceDetectedPayload},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};

#[derive(Clone)]
pub struct AppState {
    pub adb: Arc<AdbTrackerState>,
    pub control: ControlClient,
    pub pipeline: Arc<StreamPipeline>,
    pub connect_lock: Arc<tokio::sync::Mutex<()>>,
    pub quit_started: Arc<AtomicBool>,
    pub quit_ready: Arc<AtomicBool>,
    pub close_to_tray: Arc<AtomicBool>,
}

#[tauri::command]
async fn accept_usb_webcam(
    app: AppHandle,
    state: State<'_, AppState>,
    serial: String,
) -> Result<(), String> {
    if state.quit_started.load(Ordering::SeqCst) {
        return Err("Studio is closing".into());
    }
    let _guard = state.connect_lock.lock().await;
    if state.pipeline.is_running.load(Ordering::SeqCst)
        && state.pipeline.status.lock().unwrap().serial.as_ref() == Some(&serial)
    {
        let _ = state
            .control
            .send_command(serde_json::json!({"cmd":"START_STREAM"}))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    state.pipeline.cancel();
    state
        .pipeline
        .wasapi
        .lock()
        .await
        .set_enabled(false)
        .map_err(|e| e.to_string())?;
    state.control.disconnect().await;
    *state.pipeline.status.lock().unwrap() = StreamStatus {
        serial: Some(serial.clone()),
        ..Default::default()
    };
    state.pipeline.is_running.store(true, Ordering::SeqCst);
    let generation = state.pipeline.generation.load(Ordering::SeqCst);
    state
        .pipeline
        .phase(&app, "connecting", "Preparing your phone");
    let session = state.inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_session(handle.clone(), session.clone(), serial, generation).await;
        if session.pipeline.generation.load(Ordering::SeqCst) == generation {
            session.pipeline.is_running.store(false, Ordering::SeqCst);
            if let Err(e) = result {
                session.pipeline.phase(&handle, "error", &e);
                log::error!("Connection: {e}");
            }
        }
    });
    if let Some(w) = app.get_webview_window("usb-prompt") {
        let _ = w.hide();
    }
    Ok(())
}
async fn run_session(
    app: AppHandle,
    state: AppState,
    serial: String,
    generation: u64,
) -> Result<(), String> {
    adb::run(&["-s", &serial, "forward", "tcp:27183", "tcp:27183"]).await?;
    adb::run(&["-s", &serial, "forward", "tcp:27184", "tcp:27184"]).await?;
    adb::run(&["-s", &serial, "forward", "tcp:27185", "tcp:27185"]).await?;
    let package = adb::run(&["-s", &serial, "shell", "pm", "path", "com.usbwebcam.app"]).await?;
    let installed = adb::run(&[
        "-s",
        &serial,
        "shell",
        "dumpsys",
        "package",
        "com.usbwebcam.app",
    ])
    .await?;
    let installed_text = String::from_utf8_lossy(&installed.stdout);
    let installed_version = installed_text
        .split("versionCode=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    if package.stdout.is_empty() || installed_version < 7 {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let phone_apk = executable.with_file_name("Phone App").join("UWebcam.apk");
        let apk = if phone_apk.is_file() {
            phone_apk
        } else {
            executable.with_file_name("UWebcam.apk")
        };
        if !apk.is_file() {
            return Err(
                "Put UWebcam.apk in the Phone App folder beside the studio, then reconnect.".into(),
            );
        }
        adb::run(&["-s", &serial, "install", "-r", &apk.to_string_lossy()]).await?;
    }
    if state.pipeline.generation.load(Ordering::SeqCst) != generation {
        return Ok(());
    }
    adb::run(&[
        "-s",
        &serial,
        "shell",
        "am",
        "start",
        "-n",
        "com.usbwebcam.app/.MainActivity",
    ])
    .await?;
    let mut attempts = 0u32;
    while state.pipeline.generation.load(Ordering::SeqCst) == generation {
        // ADB removes forwards when a USB transport resets.
        for port in ["27183", "27184", "27185"] {
            let socket = format!("tcp:{port}");
            adb::run(&["-s", &serial, "forward", &socket, &socket]).await?;
        }
        let mut handshake = None;
        for _ in 0..20 {
            if state.pipeline.generation.load(Ordering::SeqCst) != generation {
                return Ok(());
            }
            if state.control.connect("127.0.0.1:27184").await.is_ok() {
                if let Ok(reply) = state
                    .control
                    .send_command(serde_json::json!({"cmd":"GET_STATE"}))
                    .await
                {
                    handshake = Some(reply);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        let response = handshake.ok_or_else(|| {
            "Open UWebcam on your phone and allow camera access, then try again.".to_string()
        })?;
        if let Some(phone) = response.get("telemetry") {
            state.pipeline.status.lock().unwrap().phone = Some(phone.clone());
            if phone["isStreaming"] == false {
                state
                    .control
                    .send_command(serde_json::json!({"cmd":"START_STREAM"}))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        state
            .pipeline
            .phase(&app, "connecting", "Waiting for the first camera frame");
        let connected_at = std::time::Instant::now();
        match start_stream_receiver(
            Some(app.clone()),
            state.pipeline.clone(),
            generation,
            Some(state.control.clone()),
        )
        .await
        {
            Ok(()) if state.pipeline.generation.load(Ordering::SeqCst) != generation => break,
            result => {
                if connected_at.elapsed() > std::time::Duration::from_secs(15) {
                    attempts = 0;
                }
                attempts += 1;
                log::warn!(
                    "Media interrupted (attempt {attempts}): {:?}",
                    result.as_ref().err()
                );
                state.pipeline.wasapi.lock().await.set_enabled(false).ok();
                if attempts >= 10 {
                    return Err(format!(
                        "Stream interrupted: {}. Check your phone and reconnect.",
                        result
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "socket closed".into())
                    ));
                }
                state.pipeline.phase(
                    &app,
                    "reconnecting",
                    "Connection interrupted. Reconnecting…",
                );
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
    Ok(())
}
#[tauri::command]
async fn stop_stream(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _guard = state.connect_lock.lock().await;
    state.pipeline.cancel();
    state
        .pipeline
        .wasapi
        .lock()
        .await
        .set_enabled(false)
        .map_err(|e| e.to_string())?;
    let _ = state
        .control
        .send_command(serde_json::json!({"cmd":"STOP_STREAM"}))
        .await;
    state.control.disconnect().await;
    state.pipeline.phase(&app, "idle", "Stream paused");
    Ok(())
}
#[tauri::command]
fn report_preview_issue(message: String) {
    log::warn!(
        "Preview issue: {}",
        message.chars().take(1000).collect::<String>()
    );
}
#[tauri::command]
fn get_preview_frame(state: State<'_, AppState>, sequence: u32) -> tauri::ipc::Response {
    let preview = state.pipeline.preview.lock().unwrap();
    tauri::ipc::Response::new(if preview.sequence != sequence {
        preview.bytes.clone()
    } else {
        vec![]
    })
}
#[tauri::command]
async fn get_stream_status(state: State<'_, AppState>) -> Result<StreamStatus, String> {
    if state.pipeline.is_running.load(Ordering::SeqCst) {
        let sent_us = state.pipeline.clock_origin.elapsed().as_secs_f64() * 1e6;
        if let Ok(reply) = state
            .control
            .send_command(serde_json::json!({"cmd":"GET_STATE"}))
            .await
        {
            if let Some(clock) = reply["clockUs"].as_f64() {
                state.pipeline.observe_clock(clock, sent_us);
            }
            let mut status = state.pipeline.status.lock().unwrap();
            status.phone = reply.get("telemetry").cloned();
            if reply["telemetry"]["isStreaming"] == false {
                status.phase = "paused".into();
                status.message = "Camera paused on your phone".into();
            } else if status.phase == "paused" {
                status.phase = "connecting".into();
            }
        }
    }
    Ok(state.pipeline.status.lock().unwrap().clone())
}
#[tauri::command]
async fn ignore_usb_device(
    app: AppHandle,
    state: State<'_, AppState>,
    serial: String,
) -> Result<(), String> {
    state.adb.ignored_serials.write().await.insert(serial);
    if let Some(w) = app.get_webview_window("usb-prompt") {
        let _ = w.hide();
    }
    Ok(())
}
async fn phone_command(state: &AppState, cmd: serde_json::Value) -> Result<(), String> {
    if state.quit_started.load(Ordering::SeqCst) {
        return Err("Studio is closing".into());
    }
    let reply = state
        .control
        .send_command(cmd)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(t) = reply.get("telemetry") {
        state.pipeline.status.lock().unwrap().phone = Some(t.clone());
    }
    Ok(())
}
#[tauri::command]
async fn configure_camera(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<(), String> {
    phone_command(&state, config).await
}
#[tauri::command]
async fn set_audio(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    if enabled {
        state
            .pipeline
            .wasapi
            .lock()
            .await
            .set_enabled(true)
            .map_err(|e| e.to_string())?;
    }
    if let Err(e) = phone_command(
        &state,
        serde_json::json!({"cmd":"SET_AUDIO","enabled":enabled}),
    )
    .await
    {
        state.pipeline.wasapi.lock().await.set_enabled(false).ok();
        return Err(e);
    }
    if !enabled {
        state
            .pipeline
            .wasapi
            .lock()
            .await
            .set_enabled(false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
async fn set_torch(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    phone_command(
        &state,
        serde_json::json!({"cmd":"SET_TORCH","enabled":enabled}),
    )
    .await
}
#[tauri::command]
async fn set_zoom(state: State<'_, AppState>, ratio: f64) -> Result<(), String> {
    phone_command(&state, serde_json::json!({"cmd":"SET_ZOOM","ratio":ratio})).await
}
#[tauri::command]
async fn set_exposure(state: State<'_, AppState>, compensation: i32) -> Result<(), String> {
    phone_command(
        &state,
        serde_json::json!({"cmd":"SET_EXPOSURE","compensation":compensation}),
    )
    .await
}
#[tauri::command]
fn update_filter_uniforms(
    state: State<'_, AppState>,
    uniforms: FilterUniforms,
) -> Result<(), String> {
    *state.pipeline.uniforms.lock().unwrap() = uniforms;
    // The renderer combines user adjustments with the orientation carried by
    // each frame. Avoid blocking the UI thread on GPU readback here.
    Ok(())
}
#[tauri::command]
async fn get_audio_db(state: State<'_, AppState>) -> Result<f32, String> {
    Ok(state.pipeline.wasapi.lock().await.get_db_level())
}
#[tauri::command]
async fn get_connected_devices() -> Result<Vec<UsbDeviceDetectedPayload>, String> {
    let output = adb::run(&["devices", "-l"]).await?;
    let mut devices = vec![];
    for line in String::from_utf8_lossy(&output.stdout).lines().skip(1) {
        let p: Vec<_> = line.split_whitespace().collect();
        if p.len() >= 2 && (p[1] == "device" || p[1] == "unauthorized") {
            let name = p
                .iter()
                .find_map(|v| v.strip_prefix("model:"))
                .unwrap_or("Android phone")
                .replace('_', " ");
            devices.push(UsbDeviceDetectedPayload {
                serial: p[0].into(),
                model_name: name,
                authorized: p[1] == "device",
            });
        }
    }
    Ok(devices)
}

#[tauri::command]
fn get_system_setup() -> serde_json::Value {
    use cpal::traits::{DeviceTrait, HostTrait};
    let camera = windows_registry::CLASSES_ROOT
        .open("CLSID\\{5C2CD55C-92AD-4999-8666-912BD3E70010}\\InprocServer32")
        .is_ok();
    let camera_name=windows_registry::CLASSES_ROOT.open("CLSID\\{860BB310-5D01-11D0-BD3B-00A0C911CE86}\\Instance\\{5C2CD55C-92AD-4999-8666-912BD3E70010}")
        .and_then(|key|key.get_string("FriendlyName")).unwrap_or_else(|_|"Unity Video Capture".into());
    let audio = cpal::default_host()
        .output_devices()
        .map(|mut devices| {
            devices.any(|d| d.name().map(|n| n.contains("CABLE Input")).unwrap_or(false))
        })
        .unwrap_or(false);
    serde_json::json!({"cameraInstalled":camera,"audioCableInstalled":audio,"cameraName":camera_name})
}
#[tauri::command]
fn get_desktop_preferences(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({"closeToTray":state.close_to_tray.load(Ordering::SeqCst)})
}
#[tauri::command]
fn set_close_behavior(
    app: AppHandle,
    state: State<'_, AppState>,
    close_to_tray: bool,
) -> Result<(), String> {
    let directory = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    std::fs::write(
        directory.join("preferences.json"),
        serde_json::json!({"closeToTray":close_to_tray}).to_string(),
    )
    .map_err(|e| e.to_string())?;
    state.close_to_tray.store(close_to_tray, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
async fn install_virtual_camera() -> Result<(), String> {
    let script = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("Install-Virtual-Camera.ps1");
    if !script.is_file() {
        return Err(
            "Keep Install-Virtual-Camera.ps1 and the drivers folder beside the studio executable."
                .into(),
        );
    }
    let mut process = tokio::process::Command::new("powershell.exe");
    process.creation_flags(0x08000000);
    let output = process
        .args(["-NoProfile", "-File"])
        .arg(script)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Virtual camera setup was not completed. Allow the Windows administrator prompt and try again.".into());
    }
    Ok(())
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let adb_state = Arc::new(AdbTrackerState::new());
    let app_state = AppState {
        adb: adb_state.clone(),
        control: ControlClient::new(),
        pipeline: Arc::new(StreamPipeline::new()),
        connect_lock: Arc::new(tokio::sync::Mutex::new(())),
        quit_started: Arc::new(AtomicBool::new(false)),
        quit_ready: Arc::new(AtomicBool::new(false)),
        close_to_tray: Arc::new(AtomicBool::new(false)),
    };
    tauri::Builder::default()
        .manage(app_state)
        .setup(move |app| {
            if let Ok(directory) = app.path().app_config_dir() {
                if let Ok(bytes) = std::fs::read(directory.join("preferences.json")) {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        app.state::<AppState>().close_to_tray.store(
                            value["closeToTray"].as_bool().unwrap_or(false),
                            Ordering::SeqCst,
                        );
                    }
                }
            }
            start_adb_tracker(app.handle().clone(), adb_state);
            let show = MenuItem::with_id(app, "show", "Open Ultimate Webcam", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new().menu(&menu);
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.on_menu_event(|app, e| match e.id.as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, e| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = e
                {
                    if let Some(w) = tray.app_handle().get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            })
            .build(app)?;
            Ok(())
        })
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                if w.label() == "main" {
                    api.prevent_close();
                    if w.app_handle()
                        .state::<AppState>()
                        .close_to_tray
                        .load(Ordering::SeqCst)
                    {
                        let _ = w.hide();
                    } else {
                        w.app_handle().exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            accept_usb_webcam,
            stop_stream,
            get_preview_frame,
            report_preview_issue,
            get_system_setup,
            get_desktop_preferences,
            set_close_behavior,
            install_virtual_camera,
            get_stream_status,
            ignore_usb_device,
            configure_camera,
            set_audio,
            set_torch,
            set_zoom,
            set_exposure,
            update_filter_uniforms,
            get_audio_db,
            get_connected_devices
        ])
        .build(tauri::generate_context!())
        .expect("Could not start Ultimate Webcam")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>().inner().clone();
                if state.quit_ready.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                if !state.quit_started.swap(true, Ordering::SeqCst) {
                    state.pipeline.cancel();
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let stopped = tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            crate::stream::control_client::stop_remote_camera("127.0.0.1:27184"),
                        )
                        .await;
                        log::info!("Studio shutdown: phone stop result {:?}", stopped);
                        if let Ok(mut audio) = state.pipeline.wasapi.try_lock() {
                            let _ = audio.set_enabled(false);
                        }
                        state.quit_ready.store(true, Ordering::SeqCst);
                        handle.exit(0);
                    });
                }
            }
        });
}
