use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

pub fn command() -> tokio::process::Command {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("adb.exe")));
    let path = beside_exe
        .filter(|p| p.is_file())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../adb.exe");
            p.is_file().then_some(p)
        })
        .unwrap_or_else(|| PathBuf::from("adb"));
    let mut cmd = tokio::process::Command::new(path);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW: applies to every ADB subprocess.
    cmd.kill_on_drop(true);
    cmd
}

pub async fn run(args: &[&str]) -> Result<Output, String> {
    let output = tokio::time::timeout(Duration::from_secs(20), command().args(args).output())
        .await
        .map_err(|_| {
            "ADB timed out. Check the USB cable and authorize USB debugging on the phone."
                .to_string()
        })?
        .map_err(|e| format!("Could not run bundled ADB: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(output)
}
