use std::time::SystemTime;

/// Wall-clock timestamp in milliseconds since the Unix epoch.
///
/// `SystemTime::duration_since` fails for timestamps before the epoch, which
/// happens with malformed metadata on some filesystems, so those collapse to 0.
pub fn millis(t: SystemTime) -> u64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as u64,
        Err(_) => 0,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: u64,
    pub ext: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_count: u64,
}

/// Coarse progress payload pushed while a scan walks the filesystem.
/// Emitted at most a few times per second, never per directory entry.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub current_path: String,
    pub entries_seen: u64,
    pub bytes_seen: u64,
    pub roots: Vec<RootProgress>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootProgress {
    pub path: String,
    pub bytes_seen: u64,
    pub done: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub roots: Vec<VolumeInfo>,
    pub largest_files: Vec<FileEntry>,
    pub largest_folders: Vec<FolderEntry>,
    pub total_bytes: u64,
    pub total_files: u64,
    pub unreadable_dirs: u64,
    pub elapsed_ms: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub mount: String,
    pub label: Option<String>,
    pub fs_type: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub removable: bool,
}

/// How dangerous it is to remove everything matched by a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    /// Regenerable by definition: caches, thumbnails, package-manager downloads.
    Safe,
    /// Recreated on demand but costly or slow to rebuild.
    Rebuildable,
    /// Real user data or system state. Off by default, never auto-selected.
    Caution,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JunkTarget {
    pub id: String,
    pub label: String,
    pub category: String,
    pub path: String,
    pub size: u64,
    pub entry_count: u64,
    pub risk: Risk,
    /// True when the target lives outside the user profile and needs admin rights.
    pub needs_elevation: bool,
    /// Whether the UI should start this target ticked. Computed here so the
    /// default-selection policy lives in exactly one place, next to the rules
    /// that decide risk, rather than being duplicated in the frontend.
    pub selected_by_default: bool,
    /// Why this path is considered junk, shown verbatim in the confirm dialog.
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPlan {
    pub total_reclaimable: u64,
    pub target_count: u64,
    pub requires_elevation_count: u64,
    pub by_category: Vec<CategoryTotal>,
    pub generated_at: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub size: u64,
    pub target_count: u64,
}

/// A clean instruction from the frontend.
///
/// Both sides are `#[serde(rename_all = "camelCase")]`: the struct for
/// deserialising the incoming request, and the field names, so the frontend
/// sends `allowPermanent` to match the property name it already uses.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanRequest {
    pub ids: Vec<String>,
    /// Send to the OS trash instead of unlinking. False requires `allow_permanent`.
    pub use_trash: bool,
    /// Explicit opt-in for irreversible deletion. Only honoured for paths the
    /// user can write to without elevation.
    pub allow_permanent: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanOutcome {
    pub freed_bytes: u64,
    pub removed_count: u64,
    pub skipped: Vec<CleanSkip>,
    pub failures: Vec<CleanFailure>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanSkip {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanFailure {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub home: String,
    pub separator: String,
}
