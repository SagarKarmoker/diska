use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::ipc::Channel;

use crate::clean;
use crate::error::{AppError, AppResult};
use crate::junk;
use crate::model::{
    CleanOutcome, CleanPlan, CleanRequest, JunkTarget, ScanProgress, ScanResult, SystemInfo,
};
use crate::paths;
use crate::scan::{self, ScanOptions};
use crate::volumes;

/// Cancel flag for the in-flight scan, if any. Starting a new scan supersedes the
/// previous one rather than running two walks over the same disk at once.
#[derive(Default)]
pub struct ScanState(pub Mutex<Option<Arc<AtomicBool>>>);

#[tauri::command]
pub async fn system_info() -> AppResult<SystemInfo> {
    use sysinfo::System;

    Ok(SystemInfo {
        os_name: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        os_version: System::os_version().unwrap_or_else(|| "unknown".into()),
        home: paths::home_dir().to_string_lossy().to_string(),
        separator: std::path::MAIN_SEPARATOR.to_string(),
    })
}

/// Volumes with their capacity, so the UI can show where the pressure is.
#[tauri::command]
pub async fn list_volumes() -> AppResult<Vec<crate::model::VolumeInfo>> {
    Ok(volumes::list_volumes())
}

/// Sensible default scan roots: the user profile plus any real volume root.
#[tauri::command]
pub async fn default_scan_roots() -> AppResult<Vec<String>> {
    Ok(volumes::default_scan_roots()
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

/// Walk the given roots and stream progress on `on_progress` until finished.
///
/// `top_files` and `top_folders` bound how many results are retained, which is
/// what keeps memory flat on a multi-million file disk.
#[tauri::command]
pub async fn start_scan(
    roots: Vec<String>,
    top_files: Option<usize>,
    top_folders: Option<usize>,
    min_file_size: Option<u64>,
    state: tauri::State<'_, ScanState>,
    on_progress: Channel<ScanProgress>,
) -> AppResult<ScanResult> {
    let chosen: Vec<PathBuf> = roots
        .iter()
        .map(|r| paths::expand(r))
        .filter(|p| volumes::is_scannable(p))
        .collect();

    if chosen.is_empty() {
        return Err(AppError::Invalid(
            "none of the given scan roots exist".into(),
        ));
    }

    // Cancel any previous run before registering this one.
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.0.lock() {
        if let Some(previous) = slot.take() {
            previous.store(true, Ordering::Relaxed);
        }
        *slot = Some(Arc::clone(&cancel));
    }

    let opts = ScanOptions {
        top_files: top_files.unwrap_or(500).clamp(1, 20_000),
        top_folders: top_folders.unwrap_or(300).clamp(1, 20_000),
        min_file_size: min_file_size.unwrap_or(0),
        ..Default::default()
    };

    // The walk is CPU and IO bound with no await points, so it runs on a
    // blocking thread rather than stalling the async runtime.
    let reporter: Arc<dyn Fn(ScanProgress) + Send + Sync> = Arc::new(move |p| {
        let _ = on_progress.send(p);
    });

    let mut result = tauri::async_runtime::spawn_blocking(move || {
        scan::scan(&chosen, opts, cancel, Some(reporter))
    })
    .await
    .map_err(|e| AppError::Invalid(format!("scan task failed: {e}")))?;

    result.roots = volumes::list_volumes();

    if let Ok(mut slot) = state.0.lock() {
        *slot = None;
    }

    Ok(result)
}

/// Ask the running scan to stop. The walk checks the flag on every entry, so it
/// returns quickly even on a very large tree.
#[tauri::command]
pub fn cancel_scan(state: tauri::State<'_, ScanState>) {
    if let Ok(slot) = state.0.lock() {
        if let Some(flag) = slot.as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Detect cleanable caches and junk. `include_stale` additionally sweeps for old
/// installers, stale logs and build output, which costs an extra directory walk.
#[tauri::command]
pub async fn detect_junk(
    include_stale: Option<bool>,
    // Not an Option: Tauri has no `CommandArg` impl for `Option<Channel<_>>`, so
    // the channel must be declared directly. The frontend always supplies one,
    // and progress messages are simply dropped if it stops listening.
    on_progress: Channel<junk::DetectProgress>,
) -> AppResult<Vec<JunkTarget>> {
    let include_stale = include_stale.unwrap_or(false);

    let reporter: Box<dyn Fn(junk::DetectProgress) + Send + Sync> = Box::new(move |p| {
        let _ = on_progress.send(p);
    });

    run_blocking(
        move || Ok(junk::detect_with_progress(include_stale, Some(&reporter))),
        "detect",
    )
    .await
}

/// Summarise a selection so the confirm dialog can state the exact totals.
#[tauri::command]
pub async fn preview_clean(ids: Vec<String>, include_stale: Option<bool>) -> AppResult<CleanPlan> {
    let include_stale = include_stale.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let selected: Vec<JunkTarget> = junk::detect(include_stale)
            .into_iter()
            .filter(|t| ids.iter().any(|id| id == &t.id))
            .collect();
        junk::plan(&selected)
    })
    .await
    .map_err(|e| AppError::Invalid(format!("preview task failed: {e}")))
}

/// Report what a clean would do without changing anything.
#[tauri::command]
pub async fn dry_run_clean(
    request: CleanRequest,
    include_stale: Option<bool>,
) -> AppResult<CleanOutcome> {
    let include_stale = include_stale.unwrap_or(false);
    run_blocking(
        move || clean::execute(&request, include_stale, true),
        "dry run",
    )
    .await
}

/// Actually clean. Only ids discovered by `detect_junk` are honoured, so a
/// request cannot name an arbitrary path.
#[tauri::command]
pub async fn clean(request: CleanRequest, include_stale: Option<bool>) -> AppResult<CleanOutcome> {
    let include_stale = include_stale.unwrap_or(false);
    run_blocking(
        move || clean::execute(&request, include_stale, false),
        "clean",
    )
    .await
}

/// Run a blocking task and flatten the join error together with the task's own
/// result.
///
/// `spawn_blocking` returns `Result<T, JoinError>` where `T` is itself a
/// `Result`, so the `?` below is what collapses the two layers into the single
/// `Result<T, AppError>` the command is declared to return.
async fn run_blocking<T, F>(task: F, label: &str) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| AppError::Invalid(format!("{label} task failed: {e}")))?
}
