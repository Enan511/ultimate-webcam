#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::io::Write;
struct FileLogger;
impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn || m.target().starts_with("pc_webcam_studio")
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(exe.with_file_name("studio.log"))
            {
                let _ = writeln!(
                    f,
                    "{:?} {} {}",
                    std::time::SystemTime::now(),
                    r.level(),
                    r.args()
                );
            }
        }
    }
    fn flush(&self) {}
}
static LOGGER: FileLogger = FileLogger;
fn main() {
    #[cfg(windows)]
    let instance = unsafe {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexA;
        let instance = CreateMutexA(
            None,
            false,
            windows::core::s!("Local\\UltimateWebcam.Studio"),
        )
        .ok();
        if GetLastError() == ERROR_ALREADY_EXISTS {
            use windows::Win32::UI::WindowsAndMessaging::{
                FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
            };
            if let Ok(w) = FindWindowW(None, windows::core::w!("Ultimate Webcam")) {
                let _ = ShowWindow(w, SW_RESTORE);
                let _ = SetForegroundWindow(w);
            }
            return;
        }
        instance
    };
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| {
        log::error!("Application panic: {info}");
    }));
    pc_webcam_studio_lib::run();
    #[cfg(windows)]
    if let Some(h) = instance {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(h);
        }
    }
}
