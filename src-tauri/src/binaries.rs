use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::process::{Child, Command};
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct YtDlpStartupCache {
    application_home_dir: PathBuf,
    archive_file: PathBuf,
}

static YT_DLP_STARTUP_CACHE: OnceCell<YtDlpStartupCache> = OnceCell::new();
static YT_DLP_HOLDER: OnceCell<Mutex<Child>> = OnceCell::new();

// Tauri sidecars are bundled with target-triple suffix and resolved differently in dev vs prod.
fn target_triple() -> &'static str {
    // We only ship for macOS at the moment.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { "aarch64-apple-darwin" }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { "x86_64-apple-darwin" }
}

fn resolve_sidecar(app: &AppHandle, name: &str) -> Result<PathBuf> {
    // In production the binaries live in `<AppBundle>/Contents/Resources/` (without the suffix,
    // Tauri strips it when bundling). In dev we use the binaries/ folder next to Cargo.toml.
    let resource_name = format!("{name}");
    if let Ok(p) = app.path().resolve(&resource_name, tauri::path::BaseDirectory::Resource) {
        if p.exists() {
            return Ok(p);
        }
    }
    // Dev fallback: walk up from exe to find src-tauri/binaries
    let exe = std::env::current_exe()?;
    let candidates = [
        exe.parent().map(|p| p.join(format!("{name}-{}", target_triple()))),
    ];
    for c in candidates.into_iter().flatten() {
        if c.exists() {
            return Ok(c);
        }
    }
    // Last resort: explicit path from CARGO_MANIFEST_DIR at compile-time
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dev_path = PathBuf::from(manifest_dir)
        .join("binaries")
        .join(format!("{name}-{}", target_triple()));
    if dev_path.exists() {
        return Ok(dev_path);
    }
    Err(anyhow!("sidecar binary not found: {name}"))
}

pub fn ffmpeg_path(app: &AppHandle) -> Result<PathBuf> {
    resolve_sidecar(app, "ffmpeg")
}

pub fn yt_dlp_path(app: &AppHandle) -> Result<PathBuf> {
    resolve_sidecar(app, "yt-dlp")
}

pub fn apply_yt_dlp_startup_cache(command: &mut Command) {
    let Some(cache) = YT_DLP_STARTUP_CACHE.get() else {
        return;
    };
    if !cache.application_home_dir.exists() {
        return;
    }
    command
        .env("_PYI_ARCHIVE_FILE", &cache.archive_file)
        .env("_PYI_PARENT_PROCESS_LEVEL", "0")
        .env("_PYI_APPLICATION_HOME_DIR", &cache.application_home_dir);
}

pub fn warm_yt_dlp_startup_cache(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(yt_dlp) = yt_dlp_path(&app) else {
            return;
        };
        let started = Instant::now();
        match start_yt_dlp_startup_cache(&yt_dlp).await {
            Ok(()) => tracing::info!(
                "yt-dlp startup cache ready in {:.2}s",
                started.elapsed().as_secs_f64()
            ),
            Err(e) => tracing::warn!("yt-dlp startup cache failed: {e:#}"),
        }
    });
}

pub fn stop_yt_dlp_startup_cache() {
    if let Some(holder) = YT_DLP_HOLDER.get() {
        if let Ok(mut child) = holder.lock() {
            let _ = child.start_kill();
        }
    }
}

async fn start_yt_dlp_startup_cache(yt_dlp: &Path) -> Result<()> {
    if YT_DLP_STARTUP_CACHE.get().is_some() {
        return Ok(());
    }

    let cache_root = std::env::temp_dir().join(format!(
        "parrot-ytdlp-cache-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&cache_root);
    std::fs::create_dir_all(&cache_root)?;

    let mut holder = Command::new(yt_dlp)
        .arg("--ignore-config")
        .arg("--batch-file")
        .arg("-")
        .env("TMPDIR", &cache_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let Some(application_home_dir) = wait_for_pyinstaller_dir(&cache_root).await? else {
        let _ = holder.kill().await;
        anyhow::bail!("yt-dlp PyInstaller directory was not created");
    };

    let cache = YtDlpStartupCache {
        application_home_dir,
        archive_file: yt_dlp.to_path_buf(),
    };
    if YT_DLP_STARTUP_CACHE.set(cache).is_err() {
        let _ = holder.kill().await;
        return Ok(());
    }

    if YT_DLP_HOLDER.set(Mutex::new(holder)).is_err() {
        return Ok(());
    }

    let mut command = Command::new(yt_dlp);
    command
        .arg("--ignore-config")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_yt_dlp_startup_cache(&mut command);
    let status = command.status().await?;
    if !status.success() {
        anyhow::bail!("yt-dlp startup cache warmup exited with {:?}", status.code());
    }

    Ok(())
}

async fn wait_for_pyinstaller_dir(cache_root: &Path) -> Result<Option<PathBuf>> {
    for _ in 0..80 {
        if let Some(dir) = find_pyinstaller_dir(cache_root)? {
            return Ok(Some(dir));
        }
        sleep(Duration::from_millis(250)).await;
    }
    Ok(None)
}

fn find_pyinstaller_dir(cache_root: &Path) -> Result<Option<PathBuf>> {
    if !cache_root.exists() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(cache_root)? {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() && file_name.starts_with("_MEI") {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
