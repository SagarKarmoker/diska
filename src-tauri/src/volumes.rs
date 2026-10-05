//! Volume enumeration.
//!
//! Kept separate from the Tauri command layer so it can be tested without a
//! webview: the OS data it needs comes from `sysinfo` alone.

use std::path::{Path, PathBuf};

use sysinfo::Disks;

use crate::model::VolumeInfo;
use crate::paths;

/// Below this, a mount is a system partition rather than a place the user stores
/// anything (`/boot/efi`, a swap partition, a small recovery image).
const MIN_INTERESTING_BYTES: u64 = 64 * 1024 * 1024;

/// Mounts worth showing: real capacity, not removable media, and not nested
/// inside another reported mount.
///
/// The nesting filter matters on Linux, where `/boot/efi` and `/var/lib/docker`
/// show up as separate filesystems mounted inside `/`. Showing them would make
/// the same bytes appear twice and the capacity bars would not add up.
fn interesting_mounts(disks: &Disks) -> Vec<&sysinfo::Disk> {
    let all: Vec<&sysinfo::Disk> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() >= MIN_INTERESTING_BYTES)
        .filter(|d| !d.is_removable())
        .collect();

    all.iter()
        .copied()
        .filter(|candidate| {
            !all.iter().any(|other| {
                other.mount_point() != candidate.mount_point()
                    && paths::is_within(candidate.mount_point(), other.mount_point())
            })
        })
        .collect()
}

pub fn list_volumes() -> Vec<VolumeInfo> {
    let disks = Disks::new_with_refreshed_list();

    interesting_mounts(&disks)
        .into_iter()
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            let name = d.name().to_string_lossy().to_string();
            let fs = d.file_system().to_string_lossy().to_string();

            VolumeInfo {
                mount: d.mount_point().to_string_lossy().to_string(),
                label: if name.is_empty() { None } else { Some(name) },
                fs_type: fs,
                total_bytes: total,
                available_bytes: available,
                used_bytes: total.saturating_sub(available),
                removable: false,
            }
        })
        .collect()
}

/// Roots to scan by default: the user profile, plus any top-level volume that
/// holds data outside it.
pub fn default_scan_roots() -> Vec<PathBuf> {
    let home = paths::home_dir();
    let mut roots: Vec<PathBuf> = vec![home.clone()];

    for d in interesting_mounts(&Disks::new_with_refreshed_list()) {
        let mount = d.mount_point();
        if paths::is_within(&home, mount) {
            // Already covered by scanning the profile.
            continue;
        }
        roots.push(mount.to_path_buf());
    }

    roots.sort();
    roots.dedup();
    roots
}

/// True when `root` is a sensible place to scan: an existing directory that is
/// not a symlink. Symlinked roots are refused because the walk does not follow
/// links, so scanning one would report a directory with no contents.
pub fn is_scannable(root: &Path) -> bool {
    if paths::has_traversal(root) {
        return false;
    }
    match std::fs::symlink_metadata(root) {
        // `symlink_metadata` does not resolve the link, so this is the link's
        // own type rather than its target's.
        Ok(md) => md.is_dir() && !md.is_symlink(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_are_listed_with_consistent_arithmetic() {
        for v in list_volumes() {
            assert!(v.total_bytes > 0, "{} reports zero capacity", v.mount);
            assert!(
                v.used_bytes + v.available_bytes == v.total_bytes,
                "{} used+free must equal capacity",
                v.mount
            );
        }
    }

    #[test]
    fn no_reported_volume_is_nested_in_another() {
        let volumes = list_volumes();
        for a in &volumes {
            for b in &volumes {
                if a.mount == b.mount {
                    continue;
                }
                assert!(
                    !paths::is_within(Path::new(&a.mount), Path::new(&b.mount)),
                    "{} is nested inside {}, which would double count",
                    a.mount,
                    b.mount
                );
            }
        }
    }

    #[test]
    fn default_roots_include_the_home_directory_exactly_once() {
        let roots = default_scan_roots();
        let home = paths::home_dir();

        let home_count = roots.iter().filter(|r| **r == home).count();
        assert_eq!(home_count, 1, "home appeared {home_count} times");

        for r in &roots {
            assert!(is_scannable(r), "{} is not scannable", r.display());
        }
    }

    #[test]
    fn symlinked_directories_are_not_scannable() {
        let root = std::env::temp_dir().join("diska-vol-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        #[cfg(unix)]
        {
            let link = std::env::temp_dir().join("diska-vol-test-link");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&root, &link).unwrap();
            // The link itself resolves, but its own metadata is a symlink.
            assert!(!is_scannable(&link));
            let _ = std::fs::remove_file(&link);
        }

        assert!(is_scannable(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn traversal_paths_are_rejected() {
        assert!(!is_scannable(Path::new("/home/user/../etc")));
    }
}
