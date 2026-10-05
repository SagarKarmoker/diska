# diska

A cross-platform desktop app for finding what is filling your disk and reclaiming it.

Rust + [Tauri 2](https://v2.tauri.app) backend, React + TypeScript frontend, no animation
framework and effectively no motion in the UI.

## What it does

**Overview** — capacity per volume, with mounts nested inside other mounts filtered out so
the numbers add up.

**Largest files** — walks the filesystem and lists the biggest files and folders in a
sortable, filterable table. It keeps only the top N entries rather than a full index, so
memory stays flat on a disk with millions of files. It reads metadata only; file contents
are never opened.

**Clean** — detects regenerable caches (package managers, browsers, build tools, thumbnails)
and, optionally, an aggressive sweep for old installers, stale logs and build output.
Nothing is deleted without a confirmation that lists each path and its exact size.

## Design decisions worth knowing

**Deletion is id-based, not path-based.** The frontend sends rule *ids*, never paths. The
backend re-runs detection and looks each id up, so a crafted request cannot name an arbitrary
file. See `src-tauri/src/clean.rs`.

**Trash by default.** Files move to the OS trash unless you explicitly tick "delete
permanently". Permanent deletion additionally requires `allowPermanent` in the request and is
refused outside your user profile. See `AppError::NeedsElevation`.

**Protected paths are refused at delete time.** Filesystem roots, system directories and home
folders are rejected by `paths::is_protected`, and every rule is asserted at test time to point
somewhere unprotected — otherwise the UI would advertise a path as cleanable and the delete
would then refuse it.

**Only fully regenerable items are pre-selected.** Items are tiered as `safe` (caches),
`rebuildable` (slow to regenerate) or `caution` (may hold real data). Only `safe` starts
ticked. See `junk::Risk` and `clean::defaults_to_selected`.

**No elevation.** Anything outside the user profile is reported as "needs admin" and skipped.
The app never shells out to `sudo`.

## Requirements

- Rust (stable) and Node.js 20+
- Tauri v2 system dependencies. On Debian/Ubuntu:

  ```bash
  sudo apt update && sudo apt install -y \
    libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

## Running

```bash
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # produces installers in src-tauri/target/release/bundle
```

## Tests

The Rust core is decoupled from Tauri, so it can be tested without a webview.

```bash
cd src-tauri && cargo test      # 34 tests
cd src-tauri && cargo clippy --all-targets
cd .. && pnpm build            # typechecks and bundles the frontend
```

Notable coverage: path traversal rejection, protected-path refusal, symlinks being unlinked
rather than followed, directory contents being cleared while the directory survives, and
top-N capping during a scan.

## Layout

```
src-tauri/src/
  lib.rs        Tauri builder and command registration
  commands.rs   IPC surface (thin: validation lives elsewhere)
  scan.rs       parallel filesystem walk, bounded top-N, streaming progress
  junk.rs       cache and junk rules, per-platform, with risk tiers
  clean.rs      deletion, id validation, trash-first
  paths.rs      path normalisation and the protected-path list
  volumes.rs    volume enumeration and nesting filter
  model.rs      shared serialisable types
  error.rs      error type that serialises to { kind, message }
src/
  App.tsx       shell and tabs
  components/   ScannerPanel, CleanerPanel, primitives
  lib/          typed IPC wrappers, formatting helpers
  styles.css    design tokens; transitions are disabled globally
```