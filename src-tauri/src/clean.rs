//! Deletion.
//!
//! Two properties matter more than anything else here:
//!
//! 1. **The frontend can only ask for paths this module discovered.** A clean
//!    request carries rule ids, never paths. Ids are looked up against a fresh
//!    `junk::detect` run, so a crafted request cannot name an arbitrary file.
//! 2. **Trash first, permanent only on explicit opt-in.** Permanent deletion is
//!    additionally refused for anything outside the user profile.
//!
//! Nothing is deleted without re-validating the path at delete time: a symlink
//! could have been planted in a cache directory between detection and clean.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::junk;
use crate::model::{CleanFailure, CleanOutcome, CleanRequest, CleanSkip, JunkTarget};
use crate::paths;

/// Execute a clean request, re-discovering targets first so that only paths this
/// module knows about can ever be deleted.
pub fn execute(
    request: &CleanRequest,
    include_stale: bool,
    dry_run: bool,
) -> AppResult<CleanOutcome> {
    let detected = junk::detect(include_stale);
    execute_against(request, &detected, dry_run)
}

/// Execute a clean against an explicit target list.
///
/// Splitting this out keeps the deletion logic testable against temporary
/// directories instead of the developer's real caches. The production path is
/// `execute`, which always supplies detection output.
pub fn execute_against(
    request: &CleanRequest,
    detected: &[JunkTarget],
    dry_run: bool,
) -> AppResult<CleanOutcome> {
    if request.ids.is_empty() {
        return Err(AppError::Invalid("no targets selected".into()));
    }

    // Permanent deletion requires the caller to have asked for it explicitly.
    // The frontend surfaces a second confirmation, and this check makes the
    // request self-describing: a request that omits `allowPermanent` can only
    // ever move things to the trash.
    if !request.use_trash && !request.allow_permanent {
        return Err(AppError::Invalid(
            "permanent deletion requires allowPermanent".into(),
        ));
    }

    // Only ids from our own detection are honoured. Unknown ids are reported as
    // skipped rather than erroring, so a stale UI list cannot abort the whole
    // clean.
    let mut skipped: Vec<CleanSkip> = Vec::new();
    let mut selected: Vec<&JunkTarget> = Vec::new();

    for id in &request.ids {
        match detected.iter().find(|t| &t.id == id) {
            Some(t) => selected.push(t),
            None => skipped.push(CleanSkip {
                path: id.clone(),
                reason: "no longer detected, so it was not touched".into(),
            }),
        }
    }

    let mut outcome = CleanOutcome {
        freed_bytes: 0,
        removed_count: 0,
        skipped,
        failures: Vec::new(),
    };

    for target in selected {
        let path = PathBuf::from(&target.path);

        match validate(target, &path, request) {
            Ok(()) => {}
            Err(e) => {
                outcome.skipped.push(CleanSkip {
                    path: target.path.clone(),
                    reason: e.to_string(),
                });
                continue;
            }
        }

        if dry_run {
            outcome.freed_bytes += target.size;
            outcome.removed_count += 1;
            continue;
        }

        // A directory's *contents* go, not the directory itself: caches are
        // expected to keep existing.
        let to_remove: Vec<PathBuf> = if path.is_dir() {
            match std::fs::read_dir(&path) {
                Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
                Err(e) => {
                    outcome.failures.push(CleanFailure {
                        path: target.path.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            }
        } else {
            vec![path.clone()]
        };

        if to_remove.is_empty() {
            outcome.skipped.push(CleanSkip {
                path: target.path.clone(),
                reason: "already empty".into(),
            });
            continue;
        }

        let mut freed_here = 0u64;
        let mut failed_here = false;

        for item in &to_remove {
            // Re-check every child: it was not part of the validated target.
            if let Err(e) = check_removable(item, request) {
                outcome.failures.push(CleanFailure {
                    path: item.to_string_lossy().to_string(),
                    reason: e.to_string(),
                });
                failed_here = true;
                continue;
            }

            let bytes = junk::measure(item).0;

            if request.use_trash {
                // Trash keeps the files recoverable, which is why it is the
                // default even for targets we describe as safe.
                match trash_item(item) {
                    Ok(()) => freed_here += bytes,
                    Err(e) => {
                        outcome.failures.push(CleanFailure {
                            path: item.to_string_lossy().to_string(),
                            reason: format!("trash failed: {e}"),
                        });
                        failed_here = true;
                    }
                }
            } else {
                match remove_item(item) {
                    Ok(()) => freed_here += bytes,
                    Err(e) => {
                        outcome.failures.push(CleanFailure {
                            path: item.to_string_lossy().to_string(),
                            reason: e.to_string(),
                        });
                        failed_here = true;
                    }
                }
            }

            outcome.removed_count += 1;
        }

        outcome.freed_bytes += freed_here;

        if failed_here && freed_here == 0 {
            outcome.skipped.push(CleanSkip {
                path: target.path.clone(),
                reason: "nothing could be removed".into(),
            });
        }
    }

    Ok(outcome)
}

/// Guards applied to the target as a whole, before its children are enumerated.
fn validate(target: &JunkTarget, path: &Path, request: &CleanRequest) -> AppResult<()> {
    if paths::has_traversal(path) {
        return Err(AppError::OutsideAllowedRoots(path.to_path_buf()));
    }

    if paths::is_protected(path) {
        return Err(AppError::ProtectedPath(path.to_path_buf()));
    }

    if !path.exists() {
        return Err(AppError::NotFound(path.to_path_buf()));
    }

    if target.needs_elevation {
        return Err(AppError::NeedsElevation(path.to_path_buf()));
    }

    // Permanent deletion is only ever allowed inside the user profile.
    if !request.use_trash && paths::needs_elevation(path) {
        return Err(AppError::NeedsElevation(path.to_path_buf()));
    }

    Ok(())
}

/// Guards applied to each path actually being deleted, including children of a
/// directory target that were never part of the detection pass.
fn check_removable(path: &Path, request: &CleanRequest) -> AppResult<()> {
    if paths::has_traversal(path) {
        return Err(AppError::OutsideAllowedRoots(path.to_path_buf()));
    }
    if paths::is_protected(path) || paths::is_root(path) {
        return Err(AppError::ProtectedPath(path.to_path_buf()));
    }
    if !request.use_trash && paths::needs_elevation(path) {
        return Err(AppError::NeedsElevation(path.to_path_buf()));
    }
    Ok(())
}

fn trash_item(path: &Path) -> AppResult<()> {
    // Symlinks are unlinked rather than trashed: moving the target's contents
    // would delete data the link did not own.
    if is_symlink(path) {
        return remove_item(path);
    }

    trash::delete(path).map_err(|e| AppError::io(path, std::io::Error::other(e.to_string())))
}

fn remove_item(path: &Path) -> AppResult<()> {
    let md = std::fs::symlink_metadata(path).map_err(|e| AppError::io(path, e))?;

    let result = if md.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };

    result.map_err(|e| AppError::io(path, e))
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.is_symlink())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Risk;

    fn temp_root(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("diska-clean-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn request(ids: Vec<&str>, use_trash: bool, allow_permanent: bool) -> CleanRequest {
        CleanRequest {
            ids: ids.into_iter().map(String::from).collect(),
            use_trash,
            allow_permanent,
        }
    }

    /// Build a target pointing at `dir`, the way `junk::detect` would.
    fn target_for(dir: &Path) -> JunkTarget {
        let (size, count) = junk::measure(dir);
        JunkTarget {
            id: format!("test:{}", dir.display()),
            label: dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            category: "Test".to_string(),
            path: dir.to_string_lossy().to_string(),
            size,
            entry_count: count,
            risk: Risk::Safe,
            needs_elevation: paths::needs_elevation(dir),
            selected_by_default: false,
            reason: "test target".to_string(),
        }
    }

    #[test]
    fn empty_request_is_rejected() {
        let r = request(vec![], true, false);
        assert!(execute_against(&r, &[], false).is_err());
    }

    #[test]
    fn unknown_ids_are_skipped_not_destructive() {
        let r = request(vec!["totally-made-up-id"], true, false);
        let out = execute_against(&r, &[], false).unwrap();
        assert_eq!(out.removed_count, 0);
        assert_eq!(out.freed_bytes, 0);
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].reason.contains("no longer detected"));
    }

    #[test]
    fn permanent_delete_requires_explicit_opt_in() {
        // use_trash: false without allow_permanent must be refused outright,
        // so a UI bug can never turn into an irreversible delete.
        let r = request(vec!["anything"], false, false);
        let err = execute_against(&r, &[], false).unwrap_err();
        assert!(matches!(err, AppError::Invalid(_)));
    }

    #[test]
    fn cleaning_a_directory_empties_it_but_keeps_the_directory() {
        let root = temp_root("contents-only");
        std::fs::write(root.join("a.bin"), vec![b'x'; 64]).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b.bin"), vec![b'y'; 64]).unwrap();

        let t = target_for(&root);
        assert!(t.size >= 128);

        let r = request(vec![t.id.as_str()], false, true);
        let out = execute_against(&r, &[t], false).unwrap();

        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        // The directory survives so the app that owns the cache still works.
        assert!(root.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        assert_eq!(out.freed_bytes, 128);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dry_run_leaves_the_target_intact() {
        let root = temp_root("dry-run");
        std::fs::write(root.join("a.bin"), vec![b'x'; 64]).unwrap();

        let t = target_for(&root);
        let r = request(vec![t.id.as_str()], false, true);
        let out = execute_against(&r, std::slice::from_ref(&t), true).unwrap();

        assert_eq!(out.freed_bytes, 64);
        assert_eq!(out.removed_count, 1);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cleaning_a_single_file_removes_that_file() {
        let root = temp_root("single-file");
        let victim = root.join("old.dmg");
        let bystander = root.join("keep.dmg");
        std::fs::write(&victim, vec![b'x'; 10]).unwrap();
        std::fs::write(&bystander, vec![b'y'; 10]).unwrap();

        let t = target_for(&victim);
        let r = request(vec![t.id.as_str()], false, true);
        let out = execute_against(&r, &[t], false).unwrap();

        assert!(out.failures.is_empty(), "failures: {:?}", out.failures);
        assert!(!victim.exists());
        assert!(bystander.exists(), "only the selected file should go");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_target_is_reported_not_fatal() {
        let root = temp_root("missing");
        let t = target_for(&root);
        std::fs::remove_dir_all(&root).unwrap();

        let r = request(vec![t.id.as_str()], true, false);
        let out = execute_against(&r, &[t], false).unwrap();

        assert_eq!(out.removed_count, 0);
        assert_eq!(out.skipped.len(), 1);
    }

    #[test]
    fn elevation_targets_are_skipped_rather_than_attempted() {
        // A target outside the profile is never touched, even when selected.
        let root = temp_root("elevated");
        std::fs::write(root.join("a.bin"), vec![b'x'; 8]).unwrap();

        let mut t = target_for(&root);
        t.needs_elevation = true;

        let r = request(vec![t.id.as_str()], true, false);
        let out = execute_against(&r, &[t], false).unwrap();

        assert_eq!(out.removed_count, 0);
        assert!(out.skipped[0].reason.contains("elevation"));
        assert!(root.join("a.bin").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn protected_paths_are_refused() {
        for p in ["/", "/home/sagar", "/home/sagar/Documents", "/usr"] {
            let err = check_removable(Path::new(p), &request(vec![], false, true)).unwrap_err();
            assert!(
                matches!(err, AppError::ProtectedPath(_)),
                "{p} should be protected, got {err:?}"
            );
        }
    }

    #[test]
    fn permanent_delete_outside_home_is_refused() {
        let err = check_removable(
            Path::new("/var/cache/apt/archives"),
            &request(vec![], false, true),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::NeedsElevation(_)));
    }

    #[test]
    fn traversal_in_path_is_refused() {
        let err = check_removable(
            Path::new("/home/sagar/../root/.ssh"),
            &request(vec![], true, false),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::OutsideAllowedRoots(_)));
    }

    #[test]
    fn deleting_a_directory_removes_its_contents() {
        let root = temp_root("remove-dir");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/one"), vec![b'x'; 10]).unwrap();
        assert_eq!(junk::measure(&root), (10, 1));

        remove_item(&root).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn symlink_children_are_unlinked_not_followed() {
        let root = temp_root("symlink");
        let real = temp_root("symlink-target");
        std::fs::write(real.join("precious.txt"), b"do not delete").unwrap();
        std::fs::create_dir_all(root.join("cache")).unwrap();
        std::os::unix::fs::symlink(&real, root.join("cache/link")).unwrap();

        trash_item(&root.join("cache/link")).unwrap();

        assert!(!root.join("cache/link").exists());
        // The symlink's target must be untouched.
        assert!(real.join("precious.txt").exists());
        assert_eq!(
            std::fs::read_to_string(real.join("precious.txt")).unwrap(),
            "do not delete"
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&real);
    }
}
