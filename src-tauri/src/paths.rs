use std::path::{Component, Path, PathBuf};

/// Filesystem roots that must never be handed to a delete routine, even if a
/// caller somehow produces them.
const NEVER_DELETE: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/home",
    "/lib",
    "/lib32",
    "/lib64",
    "/opt",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/srv",
    "/sys",
    "/usr",
    "/var",
    "C:\\",
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
    "C:\\ProgramData",
    "C:\\Users",
];

/// Directory names that mark a path as "this is a whole tree root" rather than a
/// junk entry. Used as a coarse guard against a rule expanding into a home dir.
const PROTECTED_DIR_NAMES: &[&str] = &[
    "home",
    "root",
    "Users",
    "user",
    "Documents",
    "Desktop",
    "Downloads",
    "Music",
    "Pictures",
    "Videos",
    "Library",
    "Applications",
    "System",
    "Windows",
    "Program Files",
    "bin",
    "boot",
    "dev",
    "etc",
    "lib",
    "lib32",
    "lib64",
    "proc",
    "sbin",
    "srv",
    "sys",
    "usr",
    "var",
];

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Lexically normalise a path: resolve `.` and collapse `..` without touching
/// the filesystem, so it is safe to call on paths that do not exist.
///
/// This must not follow symlinks. It is used purely to make path comparison and
/// prefix checks meaningful.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// True when `path` is `root` or lives underneath it, compared lexically.
pub fn is_within(path: &Path, root: &Path) -> bool {
    let p = normalize(path);
    let r = normalize(root);
    p == r || p.starts_with(&r)
}

/// True only for a filesystem root itself (`/`, `C:\`), not for any absolute
/// path. Every absolute path begins with a `RootDir` component, so the test is
/// on the component *count*, not on the first component.
pub fn is_root(path: &Path) -> bool {
    let norm = normalize(path);
    if norm.parent().is_none() {
        return true;
    }
    match norm.components().next() {
        Some(Component::RootDir) => norm.components().count() == 1,
        _ => false,
    }
}

/// Reject anything that is a filesystem root, a system directory, or a well-known
/// user home folder. Applied to every path a rule is about to clean.
pub fn is_protected(path: &Path) -> bool {
    let norm = normalize(path);

    if is_root(&norm) {
        return true;
    }

    // The user's home directory itself, even when it is not one of the
    // well-known names above.
    if norm == normalize(&home_dir()) {
        return true;
    }

    let raw = norm
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    let lowered = raw.to_ascii_lowercase();
    if NEVER_DELETE.iter().any(|n| lowered == *n) {
        return true;
    }

    // A directory whose own name is a home/system folder is off limits even when
    // it sits under an unrelated parent.
    if let Some(name) = norm.file_name().and_then(|n| n.to_str()) {
        if PROTECTED_DIR_NAMES
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name))
        {
            return true;
        }
    }

    false
}

/// Paths outside the user profile are normally root-owned. Deleting them needs
/// elevation, which this app never requests, so they are reported instead.
pub fn needs_elevation(path: &Path) -> bool {
    let home = home_dir();
    !is_within(path, &home) && !is_within(path, &std::env::temp_dir())
}

/// Reject paths that contain a `..` traversal before they are resolved, so a
/// crafted id from the frontend cannot escape the roots we vetted.
pub fn has_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Expand a single leading `~` and any `$HOME` / `$USERPROFILE` occurrences.
pub fn expand(path: &str) -> PathBuf {
    let home = home_dir();
    let raw = path.trim();

    if raw == "~" {
        return home;
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return home.join(rest);
    }

    let home_str = home.to_string_lossy().to_string();
    let expanded = raw
        .replace("$HOME", &home_str)
        .replace("${HOME}", &home_str)
        .replace("$USERPROFILE", &home_str);

    PathBuf::from(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_parent_segments() {
        assert_eq!(normalize(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalize(Path::new("/a/./b//c")), PathBuf::from("/a/b/c"));
    }

    #[test]
    fn is_within_handles_siblings_with_shared_prefix() {
        assert!(is_within(
            Path::new("/home/sagar/x"),
            Path::new("/home/sagar")
        ));
        assert!(is_within(
            Path::new("/home/sagar"),
            Path::new("/home/sagar")
        ));
        assert!(!is_within(
            Path::new("/home/sagarghost"),
            Path::new("/home/sagar")
        ));
        assert!(!is_within(
            Path::new("/etc/passwd"),
            Path::new("/home/sagar")
        ));
    }

    #[test]
    fn root_and_system_dirs_are_protected() {
        assert!(is_protected(Path::new("/")));
        assert!(is_protected(Path::new("/usr")));
        assert!(is_protected(Path::new("/home/sagar")));
        assert!(is_protected(Path::new("/home/sagar/Documents")));
        assert!(!is_protected(Path::new("/home/sagar/.cache/pip")));
    }

    #[test]
    fn traversal_is_detected_before_normalization() {
        assert!(has_traversal(Path::new("/home/sagar/../root")));
        assert!(!has_traversal(Path::new("/home/sagar/.cache")));
    }

    #[test]
    fn expand_resolves_home_prefixes() {
        let home = home_dir();
        assert_eq!(expand("~"), home);
        assert_eq!(expand("~/.cache/pip"), home.join(".cache/pip"));
        assert!(expand("$HOME/x").starts_with(&home));
    }
}
