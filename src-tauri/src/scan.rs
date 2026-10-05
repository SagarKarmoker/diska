use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignore::{WalkBuilder, WalkState};

use crate::model::{FileEntry, FolderEntry, RootProgress, ScanProgress, ScanResult};

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub top_files: usize,
    pub top_folders: usize,
    pub min_file_size: u64,
    pub progress_interval: Duration,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            top_files: 500,
            top_folders: 300,
            min_file_size: 0,
            progress_interval: Duration::from_millis(250),
        }
    }
}

/// Per-root progress: the root path, a byte counter, and a done flag.
type RootEntry = (PathBuf, Arc<AtomicU64>, Arc<AtomicBool>);

#[derive(Debug)]
struct Shared {
    opts: ScanOptions,
    cancel: Arc<AtomicBool>,
    entries_seen: AtomicU64,
    bytes_seen: AtomicU64,
    unreadable_dirs: AtomicU64,
    roots_seen: Mutex<Vec<RootEntry>>,
    active_root: Mutex<PathBuf>,
    files: Mutex<BinaryHeap<FileRank>>,
    folders: Mutex<HashMap<PathBuf, (u64, u64)>>,
}

#[derive(Debug, Eq, PartialEq)]
struct FileRank {
    size: u64,
    path: PathBuf,
    modified: u64,
}

/// Min-heap semantics via Ord reversal: we want to evict the *smallest* file once
/// the heap is full, so "greatest" must be the least valuable entry.
impl Ord for FileRank {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size
            .cmp(&other.size)
            .then_with(|| self.path.cmp(&other.path))
    }
}

impl PartialOrd for FileRank {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Shared {
    fn new(opts: ScanOptions, cancel: Arc<AtomicBool>) -> Self {
        Self {
            opts,
            cancel,
            entries_seen: AtomicU64::new(0),
            bytes_seen: AtomicU64::new(0),
            unreadable_dirs: AtomicU64::new(0),
            roots_seen: Mutex::new(Vec::new()),
            active_root: Mutex::new(PathBuf::new()),
            files: Mutex::new(BinaryHeap::with_capacity(opts.top_files + 1)),
            folders: Mutex::new(HashMap::new()),
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn record_file(&self, path: &Path, size: u64, modified: u64) {
        if size < self.opts.min_file_size {
            return;
        }
        self.bytes_seen.fetch_add(size, Ordering::Relaxed);

        let mut heap = self.files.lock().unwrap();
        if heap.len() < self.opts.top_files {
            heap.push(FileRank {
                size,
                path: path.to_path_buf(),
                modified,
            });
        } else if let Some(worst) = heap.peek() {
            if size > worst.size {
                heap.pop();
                heap.push(FileRank {
                    size,
                    path: path.to_path_buf(),
                    modified,
                });
            }
        }
    }

    /// Attribute a file to its immediate parent only. Ancestor rollup happens
    /// once at the end, so a deep tree costs one map insert per file instead of
    /// one per path segment.
    fn record_file_parent(&self, parent: &Path, size: u64) {
        let mut folders = self.folders.lock().unwrap();
        let e = folders.entry(parent.to_path_buf()).or_insert((0, 0));
        e.0 += size;
        e.1 += 1;
    }

    /// Register a directory with zero totals so empty dirs still appear in the
    /// folder list rather than being silently absent.
    fn record_empty_dir(&self, path: &Path) {
        let mut folders = self.folders.lock().unwrap();
        folders.entry(path.to_path_buf()).or_insert((0, 0));
    }

    /// Roll immediate-parent totals up through every ancestor, deepest first so
    /// each directory is complete before it contributes to its own parent.
    /// Afterwards each entry holds the recursive size and file count.
    fn roll_up_folders(&self) {
        let mut folders = self.folders.lock().unwrap();

        let mut by_depth: Vec<PathBuf> = folders.keys().cloned().collect();
        by_depth.sort_by_key(|p| std::cmp::Reverse(p.components().count()));

        for path in by_depth {
            let Some((size, count)) = folders.get(&path).copied() else {
                continue;
            };
            let Some(parent) = path.parent() else {
                continue;
            };
            let e = folders.entry(parent.to_path_buf()).or_insert((0, 0));
            e.0 += size;
            e.1 += count;
        }
    }

    fn mark_root_done(&self, root: &Path) {
        if let Ok(roots) = self.roots_seen.lock() {
            for (p, _, done) in roots.iter() {
                if p == root {
                    done.store(true, Ordering::Relaxed);
                }
            }
        }
    }

    fn root_counter(&self, root: &Path) -> Option<Arc<AtomicU64>> {
        self.roots_seen
            .lock()
            .ok()?
            .iter()
            .find(|(p, _, _)| p == root)
            .map(|(_, c, _)| Arc::clone(c))
    }
}

/// Walk `roots` concurrently, returning the largest files and folders plus totals.
///
/// Progress is pushed on `on_progress` at most once per `opts.progress_interval`.
pub fn scan(
    roots: &[PathBuf],
    opts: ScanOptions,
    cancel: Arc<AtomicBool>,
    on_progress: Option<Arc<dyn Fn(ScanProgress) + Send + Sync>>,
) -> ScanResult {
    let started = Instant::now();
    let shared = Arc::new(Shared::new(opts, cancel.clone()));

    for root in roots {
        let counter = Arc::new(AtomicU64::new(0));
        shared.roots_seen.lock().unwrap().push((
            root.clone(),
            counter,
            Arc::new(AtomicBool::new(false)),
        ));
    }

    // A cheap sampler thread drives progress emission so the walk threads stay
    // free of reporting work.
    let reporter = on_progress.map(|cb| {
        let shared = Arc::clone(&shared);
        let interval = opts.progress_interval;
        std::thread::spawn(move || {
            while !shared.cancelled() {
                let roots: Vec<RootProgress> = shared
                    .roots_seen
                    .lock()
                    .map(|rs| {
                        rs.iter()
                            .map(|(p, c, d)| RootProgress {
                                path: p.to_string_lossy().to_string(),
                                bytes_seen: c.load(Ordering::Relaxed),
                                done: d.load(Ordering::Relaxed),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let current = shared
                    .active_root
                    .lock()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                cb(ScanProgress {
                    phase: "scanning".into(),
                    current_path: current,
                    entries_seen: shared.entries_seen.load(Ordering::Relaxed),
                    bytes_seen: shared.bytes_seen.load(Ordering::Relaxed),
                    roots,
                });
                std::thread::sleep(interval);
            }
        })
    });

    let walk = {
        let shared = Arc::clone(&shared);
        let roots: Vec<PathBuf> = roots.to_vec();
        std::thread::spawn(move || {
            for root in &roots {
                if shared.cancelled() {
                    break;
                }
                walk_root(&shared, root);
                shared.mark_root_done(root);
            }
        })
    };

    let _ = walk.join();
    cancel.store(true, Ordering::Relaxed);
    if let Some(h) = reporter {
        let _ = h.join();
    }

    build_result(&shared, started, opts)
}

fn walk_root(shared: &Arc<Shared>, root: &Path) {
    let root_counter = shared.root_counter(root);
    // Owned copy: the visitor closure is `'static`, so it cannot borrow `root`.
    let root_owned = root.to_path_buf();
    if let Ok(mut active) = shared.active_root.lock() {
        *active = root_owned.clone();
    }

    // The builder's setters borrow `&mut self` while `run` consumes it, so the
    // options are applied as statements rather than chained.
    let mut builder = WalkBuilder::new(&root_owned);
    // A disk analyzer must see dotfiles and ignore files: hidden and
    // gitignored paths are exactly the ones eating space.
    builder
        .standard_filters(false)
        .hidden(false)
        .parents(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(false)
        // Stay on one filesystem so bind mounts and `/proc`-style recursion
        // cannot double-count or loop.
        .same_file_system(true)
        .max_depth(None)
        .threads(0);

    builder.build_parallel().run(|| {
        let shared = Arc::clone(shared);
        let root_counter = root_counter.clone();
        let root = root_owned.clone();
        Box::new(move |result: Result<ignore::DirEntry, ignore::Error>| {
            if shared.cancelled() {
                return WalkState::Quit;
            }

            let entry = match result {
                Ok(e) => e,
                Err(_) => {
                    shared.unreadable_dirs.fetch_add(1, Ordering::Relaxed);
                    return WalkState::Continue;
                }
            };

            // `None` only happens for stdin, which never appears in a real walk.
            let Some(file_type) = entry.file_type() else {
                return WalkState::Continue;
            };

            // Never follow or count symlink targets: that double counts and can
            // walk straight out of the root.
            if file_type.is_symlink() {
                return WalkState::Skip;
            }

            let path = entry.path();
            shared.entries_seen.fetch_add(1, Ordering::Relaxed);

            if file_type.is_dir() {
                // Registering every directory guarantees each parent already
                // exists in the folder map before rollup runs.
                if path != root {
                    shared.record_empty_dir(path);
                }
                return WalkState::Continue;
            }

            let Ok(md) = entry.metadata() else {
                shared.unreadable_dirs.fetch_add(1, Ordering::Relaxed);
                return WalkState::Continue;
            };

            let size = md.len();
            let modified = md.modified().map(crate::model::millis).unwrap_or(0);

            shared.record_file(path, size, modified);
            if let Some(c) = &root_counter {
                c.fetch_add(size, Ordering::Relaxed);
            }

            // Only the immediate parent is recorded here; `roll_up_folders`
            // propagates totals to ancestors once the walk is done.
            if let Some(parent) = path.parent() {
                shared.record_file_parent(parent, size);
            }

            WalkState::Continue
        })
    })
}

fn build_result(shared: &Arc<Shared>, started: Instant, opts: ScanOptions) -> ScanResult {
    shared.roll_up_folders();

    let mut largest_files: Vec<FileEntry> = shared
        .files
        .lock()
        .unwrap()
        .drain()
        .map(|f| FileEntry {
            name: f
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| f.path.to_string_lossy().to_string()),
            path: f.path.to_string_lossy().to_string(),
            size: f.size,
            modified: f.modified,
            ext: f
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
        })
        .collect();
    largest_files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    let truncated = largest_files.len() >= opts.top_files;

    let mut folders: Vec<(PathBuf, (u64, u64))> = shared
        .folders
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    folders.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));

    let largest_folders: Vec<FolderEntry> = folders
        .into_iter()
        .take(opts.top_folders)
        .map(|(path, (size, file_count))| FolderEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            path: path.to_string_lossy().to_string(),
            size,
            file_count,
        })
        .collect();

    ScanResult {
        roots: Vec::new(),
        largest_files,
        largest_folders,
        total_bytes: shared.bytes_seen.load(Ordering::Relaxed),
        total_files: shared.entries_seen.load(Ordering::Relaxed),
        unreadable_dirs: shared.unreadable_dirs.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("diska-test-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn finds_largest_files_and_rolls_up_folders() {
        let root = scratch("scan-basic");
        write(&root.join("big.bin"), 5000);
        write(&root.join("nested/medium.bin"), 2000);
        write(&root.join("nested/deep/small.bin"), 10);
        write(&root.join(".hidden-hidden-file"), 4000);

        let res = scan(
            std::slice::from_ref(&root),
            ScanOptions::default(),
            Arc::new(AtomicBool::new(false)),
            None,
        );

        let top: Vec<&str> = res.largest_files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(top[0], "big.bin");
        assert_eq!(res.largest_files[0].size, 5000);
        // Hidden files must be included, otherwise the scan misses real usage.
        assert!(top.contains(&".hidden-hidden-file"));

        // Every ancestor of a file must carry that file's size: the rollup walks all
        // the way up, not just one level.
        let nested = res
            .largest_folders
            .iter()
            .find(|f| f.path.ends_with("nested"))
            .expect("nested folder");
        assert_eq!(nested.size, 2010);

        let deep = res
            .largest_folders
            .iter()
            .find(|f| f.path.ends_with("deep"))
            .expect("deep folder");
        assert_eq!(deep.size, 10);
        assert_eq!(deep.file_count, 1);

        // The scan root itself rolls up all 11010 bytes.
        let root_folder = res
            .largest_folders
            .iter()
            .find(|f| f.path == root.to_string_lossy())
            .expect("root folder");
        assert_eq!(root_folder.size, 11010);
        assert_eq!(root_folder.file_count, 4);

        assert_eq!(res.total_bytes, 11010);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn respects_min_file_size_and_top_cap() {
        let root = scratch("scan-cap");
        for i in 0..10 {
            write(&root.join(format!("f{i}.bin")), (i + 1) * 100);
        }

        let res = scan(
            std::slice::from_ref(&root),
            ScanOptions {
                top_files: 3,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
            None,
        );

        assert_eq!(res.largest_files.len(), 3);
        assert_eq!(res.largest_files[0].size, 1000);
        assert!(res.truncated);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pre_cancelled_scan_yields_nothing() {
        let root = scratch("scan-cancel");
        write(&root.join("a.bin"), 100);
        write(&root.join("b.bin"), 200);

        // Set before the walk starts: the walker checks the flag on every entry,
        // so it must bail out without recording anything.
        let res = scan(
            std::slice::from_ref(&root),
            ScanOptions::default(),
            Arc::new(AtomicBool::new(true)),
            None,
        );

        assert_eq!(res.total_bytes, 0);
        assert!(res.largest_files.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_reported_not_panicked() {
        let res = scan(
            &[PathBuf::from("/definitely/not/here/at/all")],
            ScanOptions::default(),
            Arc::new(AtomicBool::new(false)),
            None,
        );
        assert_eq!(res.total_bytes, 0);
        assert!(res.unreadable_dirs >= 1);
    }
}
