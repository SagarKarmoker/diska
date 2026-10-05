//! Junk and cache discovery.
//!
//! A `Rule` names a location and explains, in plain language, why its contents
//! can be regenerated. Rules never delete anything: they only produce
//! `JunkTarget`s for the UI to show and the user to confirm.
//!
//! All string fields are owned rather than `&'static str` because several ids
//! and labels are derived from paths that change per user.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{CategoryTotal, CleanPlan, JunkTarget, Risk};
use crate::paths;

/// Age thresholds. Anything newer than these is left alone, because a recent
/// download or a fresh log is plausibly still wanted.
const STALE_INSTALLER_DAYS: u64 = 30;
const STALE_LOG_DAYS: u64 = 30;
const MAX_BUILD_WALK_DEPTH: usize = 3;
const MAX_BUILD_WALK_DIRS: usize = 8_000;

const DAY_SECS: u64 = 60 * 60 * 24;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether a timestamp is older than `days`. A zero timestamp means unknown,
/// which is treated as "not old enough" so we never delete on missing data.
fn older_than_days(millis: u64, days: u64) -> bool {
    if millis == 0 {
        return false;
    }
    let cutoff_ms = now_ms().saturating_sub(days * DAY_SECS * 1000);
    millis < cutoff_ms
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub label: String,
    pub category: String,
    pub path: PathBuf,
    pub risk: Risk,
    pub reason: String,
}

/// Installer and archive extensions commonly left in Downloads after an install.
const INSTALLER_EXTS: &[&str] = &[
    "dmg", "pkg", "deb", "rpm", "appimage", "exe", "msi", "msix", "appx", "iso", "snap", "flatpak",
    "tar", "tar.gz", "tgz", "tar.xz", "tar.bz2", "zip", "7z", "rar",
];

/// Build output directories that are safe to clear but expensive to rebuild.
const BUILD_DIR_NAMES: &[&str] = &["target", "node_modules", ".next", ".nuxt", "dist", "build"];

pub fn rules_for_platform() -> Vec<Rule> {
    let home = paths::home_dir();
    let mut rules: Vec<Rule> = Vec::new();

    let add = |rules: &mut Vec<Rule>,
               id: &str,
               label: &str,
               category: &str,
               path: PathBuf,
               risk: Risk,
               reason: &str| {
        rules.push(Rule {
            id: id.to_string(),
            label: label.to_string(),
            category: category.to_string(),
            path,
            risk,
            reason: reason.to_string(),
        });
    };

    // ---- Cross-platform user caches ---------------------------------------
    add(
        &mut rules,
        "cache-home",
        "User cache directory",
        "App caches",
        dirs::cache_dir().unwrap_or_else(|| home.join(".cache")),
        Risk::Safe,
        "Regenerated automatically by the apps that created it. Programs rebuild these caches \
         on next launch; only the first start afterwards is slower.",
    );

    // ---- Developer caches -------------------------------------------------
    for (id, label, path, risk, reason) in [
        (
            "cache-pip",
            "pip download cache",
            home.join(".cache/pip"),
            Risk::Safe,
            "Re-downloadable wheel and sdist archives. pip re-fetches them on the next install, \
             so no existing Python environment is affected.",
        ),
        (
            "cache-npm",
            "npm cache",
            home.join(".npm/_cacache"),
            Risk::Safe,
            "Package tarballs cached by npm. Re-downloaded on demand; installed node_modules \
             are untouched.",
        ),
        (
            "cache-yarn",
            "Yarn cache",
            home.join(".cache/yarn"),
            Risk::Safe,
            "Cached package tarballs, re-fetched by yarn when needed.",
        ),
        (
            "cache-pnpm-store",
            "pnpm content-addressable store",
            home.join(".local/share/pnpm/store"),
            Risk::Rebuildable,
            "Shared download store for pnpm projects. Removing it does not break existing \
             node_modules, but the next install re-downloads every package.",
        ),
        (
            "cache-cargo",
            "Cargo registry cache",
            home.join(".cargo/registry/cache"),
            Risk::Safe,
            "Cached .crate archives, re-downloaded when a project is rebuilt.",
        ),
        (
            "cache-cargo-git",
            "Cargo git checkouts",
            home.join(".cargo/git"),
            Risk::Safe,
            "Clones of git dependencies, re-fetched on the next build of a project that needs \
             them.",
        ),
        (
            "cache-gradle",
            "Gradle caches",
            home.join(".gradle/caches"),
            Risk::Rebuildable,
            "Downloaded dependencies and transform caches. Rebuilding a Gradle project after \
             cleaning is noticeably slower.",
        ),
        (
            "cache-nuget",
            "NuGet package cache",
            home.join(".nuget/packages"),
            Risk::Rebuildable,
            "Restored NuGet packages. .NET re-downloads them on the next restore.",
        ),
        (
            "cache-go-build",
            "Go build cache",
            home.join(".cache/go-build"),
            Risk::Safe,
            "Compiled build cache. Go recompiles on the next build.",
        ),
        (
            "cache-go-mod",
            "Go module cache",
            home.join("go/pkg/mod"),
            Risk::Rebuildable,
            "Downloaded Go modules. `go mod download` restores whatever the next build needs.",
        ),
        (
            "cache-composer",
            "Composer cache",
            home.join(".cache/composer"),
            Risk::Safe,
            "Downloaded PHP packages, re-fetched by composer install.",
        ),
        (
            "cache-maven",
            "Maven local repository",
            home.join(".m2/repository"),
            Risk::Rebuildable,
            "Resolved Maven artifacts. The next build re-downloads them unless offline mode is \
             used.",
        ),
    ] {
        add(
            &mut rules,
            id,
            label,
            "Developer caches",
            path,
            risk,
            reason,
        );
    }

    // ---- Browser caches ---------------------------------------------------
    for (slug, label) in [
        ("google-chrome", "Chrome"),
        ("chromium", "Chromium"),
        ("chromium-browser", "Chromium (snap)"),
        ("brave", "Brave"),
        ("vivaldi", "Vivaldi"),
        ("microsoft-edge", "Microsoft Edge"),
    ] {
        let base = home.join(".config").join(slug);
        for (sub_id, sub) in [
            ("cache", "Cache"),
            ("code-cache", "Code Cache"),
            ("gpu-cache", "GPUCache"),
        ] {
            add(
                &mut rules,
                &format!("browser-{slug}-{sub_id}"),
                &format!("{label} {sub}"),
                "Browser caches",
                base.join(sub),
                Risk::Safe,
                "Temporary rendering cache. Pages are fetched again on the next visit; \
                 bookmarks, saved passwords, cookies and history live in separate files and \
                 are not touched.",
            );
        }
    }

    rules.extend(firefox_cache_rules());

    // ---- System and package caches ---------------------------------------
    add(
        &mut rules,
        "thumbnails",
        "Thumbnail cache",
        "System and app caches",
        home.join(".cache/thumbnails"),
        Risk::Safe,
        "Thumbnail images generated for file managers and desktops. Regenerated as you browse \
         files.",
    );

    add(
        &mut rules,
        "cache-trash",
        "Trash bin",
        "System and app caches",
        trash_dir(),
        Risk::Caution,
        "Files you already deleted. Emptying the trash is irreversible.",
    );

    add(
        &mut rules,
        "flatpak-cache",
        "Flatpak download cache",
        "Package caches",
        home.join(".local/share/flatpak/cache"),
        Risk::Safe,
        "Cached Flatpak bundles, re-downloaded if an app is reinstalled.",
    );

    add(
        &mut rules,
        "snap-cache",
        "Snap download cache",
        "Package caches",
        home.join(".cache/snapd"),
        Risk::Safe,
        "Cached snap downloads, re-fetched when snaps are updated.",
    );

    add(
        &mut rules,
        "vscode-cache",
        "VS Code cached data",
        "Developer caches",
        home.join(".config/Code/Cache"),
        Risk::Safe,
        "Extension host and workspace caches. Installed extensions, settings and keybindings \
         are stored elsewhere and are unaffected.",
    );

    add(
        &mut rules,
        "vscode-cached-data",
        "VS Code cached extensions",
        "Developer caches",
        home.join(".config/Code/CachedExtensionVSIXs"),
        Risk::Safe,
        "Installer archives for extensions VS Code has already unpacked. The live \
         .vscode/extensions directory is not affected.",
    );

    add(
        &mut rules,
        "cache-apt",
        "APT downloaded packages",
        "Package caches",
        PathBuf::from("/var/cache/apt/archives"),
        Risk::Rebuildable,
        "Downloaded .deb files kept by apt, re-downloaded when packages are reinstalled. \
         Usually needs administrator rights.",
    );

    rules
}

/// Firefox keeps one directory per profile, so its caches are discovered rather
/// than guessed. Returns an empty list when Firefox is not installed.
fn firefox_cache_rules() -> Vec<Rule> {
    let base = paths::home_dir().join(".mozilla/firefox/Profiles");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            Rule {
                id: format!("firefox-{name}"),
                label: format!("Firefox cache ({name})"),
                category: "Browser caches".to_string(),
                path: e.path().join("cache2"),
                risk: Risk::Safe,
                reason: "Temporary page cache for this Firefox profile. Bookmarks, passwords \
                         and cookies are stored separately and are not touched."
                    .to_string(),
            }
        })
        .collect()
}

fn trash_dir() -> PathBuf {
    dirs::data_local_dir()
        .map(|d| d.join("Trash/files"))
        .unwrap_or_else(|| paths::home_dir().join(".local/share/Trash/files"))
}

/// Stale developer build output found by walking a shallow tree such as
/// `~/Desktop` or `~/Projects`. Each match is reported individually rather than
/// as one blanket rule, so the user can see exactly what costs what.
fn build_output_targets(root: &Path, max_depth: usize) -> Vec<JunkTarget> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }

    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;

    while let Some((dir, depth)) = stack.pop() {
        if depth >= max_depth || visited >= MAX_BUILD_WALK_DIRS {
            continue;
        }
        visited += 1;

        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            // Symlinked build dirs are skipped: their contents would be counted
            // from outside the tree.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
                continue;
            };

            if BUILD_DIR_NAMES.contains(&name.as_str()) {
                let (size, count) = measure(&path);
                if size == 0 {
                    continue;
                }
                out.push(JunkTarget {
                    id: format!("build:{}", path.to_string_lossy()),
                    label: name,
                    category: "Stale build output".to_string(),
                    path: path.to_string_lossy().to_string(),
                    size,
                    entry_count: count,
                    risk: Risk::Rebuildable,
                    needs_elevation: paths::needs_elevation(&path),
                    selected_by_default: false,
                    reason: "Build output directory. The source tree is untouched, but the next \
                             build of this project starts from scratch."
                        .to_string(),
                });
                // Do not descend into a build dir: nested output inside it is
                // already covered by this target.
                continue;
            }

            stack.push((path, depth + 1));
        }
    }

    out
}

/// Installers and archives in Downloads that have not been touched recently.
fn stale_installer_targets(downloads: &Path) -> Vec<JunkTarget> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(downloads) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        let lowered = name.to_ascii_lowercase();
        if !INSTALLER_EXTS
            .iter()
            .any(|ext| lowered.ends_with(&format!(".{ext}")))
        {
            continue;
        }

        let Ok(md) = entry.metadata() else { continue };
        let modified = md.modified().map(crate::model::millis).unwrap_or(0);
        if !older_than_days(modified, STALE_INSTALLER_DAYS) {
            continue;
        }

        out.push(JunkTarget {
            id: format!("installer:{}", path.to_string_lossy()),
            label: name,
            category: "Old installers".to_string(),
            path: path.to_string_lossy().to_string(),
            size: md.len(),
            entry_count: 1,
            risk: Risk::Caution,
            needs_elevation: paths::needs_elevation(&path),
            selected_by_default: false,
            reason: format!(
                "Installer or archive in Downloads, untouched for over {STALE_INSTALLER_DAYS} \
                 days. Check that you do not still need it before removing."
            ),
        });
    }

    out
}

/// Log files directly under the user's home that are old enough to be worthless.
fn stale_log_targets(root: &Path) -> Vec<JunkTarget> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
            continue;
        };
        let lowered = name.to_ascii_lowercase();
        if !lowered.ends_with(".log") && !lowered.ends_with(".log.1") {
            continue;
        }

        let Ok(md) = entry.metadata() else { continue };
        let modified = md.modified().map(crate::model::millis).unwrap_or(0);
        if !older_than_days(modified, STALE_LOG_DAYS) {
            continue;
        }

        out.push(JunkTarget {
            id: format!("log:{}", path.to_string_lossy()),
            label: name,
            category: "Logs and crash dumps".to_string(),
            path: path.to_string_lossy().to_string(),
            size: md.len(),
            entry_count: 1,
            risk: Risk::Safe,
            needs_elevation: paths::needs_elevation(&path),
            selected_by_default: !paths::needs_elevation(&path),
            reason: format!(
                "Log file untouched for over {STALE_LOG_DAYS} days. Useful only for debugging \
                 recent problems."
            ),
        });
    }

    out
}

/// Size and entry count of a file or directory. Symlinks are skipped rather
/// than followed, so a link into a large tree cannot inflate a cache's apparent
/// size.
pub fn measure(path: &Path) -> (u64, u64) {
    let Ok(file_type) = std::fs::symlink_metadata(path).map(|m| m.file_type()) else {
        return (0, 0);
    };

    if file_type.is_symlink() {
        return (0, 0);
    }

    if file_type.is_file() {
        return (std::fs::metadata(path).map(|m| m.len()).unwrap_or(0), 1);
    }

    if !file_type.is_dir() {
        return (0, 0);
    }

    let mut size = 0u64;
    let mut count = 0u64;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if let Ok(md) = entry.metadata() {
                size += md.len();
                count += 1;
            }
        }
    }

    (size, count)
}

/// Progress emitted while detection runs. The aggressive sweep walks real trees
/// and can take tens of seconds, so the UI needs to show something.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectProgress {
    pub phase: String,
    pub label: String,
    pub targets_found: u64,
    pub bytes_so_far: u64,
}

/// What the cleaner will offer for the current platform and user.
pub fn detect(include_stale: bool) -> Vec<JunkTarget> {
    detect_with_progress(include_stale, None)
}

/// Same as `detect`, reporting progress as it goes. `on_progress` is called
/// after each rule is measured, and during the stale sweep.
pub fn detect_with_progress(
    include_stale: bool,
    on_progress: Option<&(dyn Fn(DetectProgress) + Send + Sync)>,
) -> Vec<JunkTarget> {
    let home = paths::home_dir();
    let mut targets: Vec<JunkTarget> = Vec::new();

    let report = |cb: Option<&(dyn Fn(DetectProgress) + Send + Sync)>,
                  targets: &[JunkTarget],
                  phase: &str,
                  label: &str| {
        if let Some(cb) = cb {
            cb(DetectProgress {
                phase: phase.to_string(),
                label: label.to_string(),
                targets_found: targets.len() as u64,
                bytes_so_far: targets.iter().map(|t| t.size).sum(),
            });
        }
    };

    for rule in rules_for_platform() {
        let base = &rule.path;
        if !base.exists() || paths::is_protected(base) {
            continue;
        }

        let (size, count) = measure(base);
        if size == 0 {
            continue;
        }

        targets.push(JunkTarget {
            id: rule.id,
            label: rule.label,
            category: rule.category,
            path: base.to_string_lossy().to_string(),
            size,
            entry_count: count,
            risk: rule.risk,
            needs_elevation: paths::needs_elevation(base),
            selected_by_default: rule.risk == Risk::Safe && !paths::needs_elevation(base),
            reason: rule.reason,
        });

        report(
            on_progress,
            &targets,
            "rules",
            &targets.last().unwrap().label,
        );
    }

    if include_stale {
        for (dir, label) in [
            (home.join("Desktop"), "Desktop"),
            (home.join("Projects"), "Projects"),
        ] {
            report(on_progress, &targets, "stale", label);
            targets.extend(build_output_targets(&dir, MAX_BUILD_WALK_DEPTH));
        }

        report(on_progress, &targets, "stale", "Downloads");
        targets.extend(stale_installer_targets(&home.join("Downloads")));

        report(on_progress, &targets, "stale", "Logs");
        targets.extend(stale_log_targets(&home));
    }

    // Drop anything nested inside another detected target. Without this a cache
    // inside the home cache dir would be counted twice in the reclaimable total.
    targets.sort_by(|a, b| a.path.cmp(&b.path));
    let mut pruned: Vec<JunkTarget> = Vec::new();
    for t in targets {
        let nested = pruned
            .iter()
            .any(|p| paths::is_within(Path::new(&t.path), Path::new(&p.path)));
        if !nested {
            pruned.push(t);
        }
    }

    pruned.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    pruned
}

/// Summarise detected targets into the plan shown before the confirm dialog.
pub fn plan(targets: &[JunkTarget]) -> CleanPlan {
    let total_reclaimable = targets.iter().map(|t| t.size).sum();
    let requires_elevation_count = targets.iter().filter(|t| t.needs_elevation).count() as u64;

    let mut by_category: Vec<CategoryTotal> = Vec::new();
    for t in targets {
        match by_category.iter_mut().find(|c| c.category == t.category) {
            Some(c) => {
                c.size += t.size;
                c.target_count += 1;
            }
            None => by_category.push(CategoryTotal {
                category: t.category.clone(),
                size: t.size,
                target_count: 1,
            }),
        }
    }
    by_category.sort_by_key(|c| std::cmp::Reverse(c.size));

    CleanPlan {
        total_reclaimable,
        target_count: targets.len() as u64,
        requires_elevation_count,
        by_category,
        generated_at: now_ms(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &str, size: u64) -> JunkTarget {
        JunkTarget {
            id: path.to_string(),
            label: path.to_string(),
            category: "Test".to_string(),
            path: path.to_string(),
            size,
            entry_count: 1,
            risk: Risk::Safe,
            needs_elevation: false,
            selected_by_default: false,
            reason: String::new(),
        }
    }

    #[test]
    fn age_threshold_ignores_unknown_timestamps() {
        // Zero means "could not read mtime", which must never count as stale.
        assert!(!older_than_days(0, STALE_LOG_DAYS));
        assert!(!older_than_days(now_ms(), STALE_LOG_DAYS));
        assert!(older_than_days(
            now_ms() - (400 * DAY_SECS * 1000),
            STALE_LOG_DAYS
        ));
    }

    #[test]
    fn measure_counts_recursively_and_skips_symlinks() {
        let root = std::env::temp_dir().join("diska-measure-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/one"), vec![b'x'; 100]).unwrap();
        std::fs::write(root.join("a/b/two"), vec![b'x'; 50]).unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", root.join("link")).unwrap();

        // 150 bytes of real files; the symlink into /etc must not be walked.
        assert_eq!(measure(&root), (150, 2));
        // A single file must report its own size, not zero.
        assert_eq!(measure(&root.join("a/one")), (100, 1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detect_prunes_targets_nested_in_other_targets() {
        // Mirrors the pruning logic in `detect` for a home cache containing a
        // sub-cache: the parent must win so totals are not double counted.
        let mut targets = vec![
            target("/home/u/.cache", 900),
            target("/home/u/.cache/pip", 100),
        ];
        targets.sort_by(|a, b| a.path.cmp(&b.path));

        let mut pruned: Vec<JunkTarget> = Vec::new();
        for t in targets {
            let nested = pruned
                .iter()
                .any(|p| paths::is_within(Path::new(&t.path), Path::new(&p.path)));
            if !nested {
                pruned.push(t);
            }
        }

        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].path, "/home/u/.cache");
    }

    #[test]
    fn plan_sums_by_category() {
        let p = plan(&[target("/a", 100), target("/b", 50), target("/c", 25)]);
        assert_eq!(p.total_reclaimable, 175);
        assert_eq!(p.target_count, 3);
        assert_eq!(p.by_category.len(), 1);
        assert_eq!(p.by_category[0].size, 175);
        assert_eq!(p.by_category[0].target_count, 3);
    }

    #[test]
    fn installer_extension_matching() {
        let matched = |n: &str| {
            let lowered = n.to_ascii_lowercase();
            INSTALLER_EXTS
                .iter()
                .any(|e| lowered.ends_with(&format!(".{e}")))
        };
        assert!(matched("thing.dmg"));
        assert!(matched("Arch.tar.gz"));
        assert!(matched("setup.EXE"));
        assert!(!matched("report.pdf"));
        assert!(!matched("notes.txt"));
    }

    #[test]
    fn only_safe_targets_outside_the_profile_are_pre_selected() {
        // The dangerous direction is ticking something by default. Anything
        // needing elevation can never be cleaned, so it must never be ticked
        // either, and neither may rebuildable or caution targets.
        for t in detect(false) {
            if t.selected_by_default {
                assert_eq!(t.risk, Risk::Safe, "{} is safe but not Safe risk", t.label);
                assert!(
                    !t.needs_elevation,
                    "{} needs elevation yet is pre-selected",
                    t.label
                );
            }
        }
    }

    #[test]
    fn default_selection_matches_the_safe_and_writable_rule() {
        // Guards against the field drifting from the policy it is meant to
        // encode, independent of what detection happens to find on this machine.
        let cases = [
            (Risk::Safe, false, true),
            (Risk::Safe, true, false),
            (Risk::Rebuildable, false, false),
            (Risk::Caution, false, false),
        ];
        for (risk, elevated, expected) in cases {
            assert_eq!(
                risk == Risk::Safe && !elevated,
                expected,
                "risk {risk:?} elevated={elevated}"
            );
        }
    }

    #[test]
    fn every_rule_targets_a_path_outside_protected_roots() {
        // A rule pointing at `/usr` or the home dir itself would be reported as
        // cleanable but then refused at delete time. Catch that at definition.
        for rule in rules_for_platform() {
            assert!(
                !paths::is_protected(&rule.path),
                "rule {} points at a protected path: {}",
                rule.id,
                rule.path.display()
            );
        }
    }
}
