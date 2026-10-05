import { Channel, invoke } from "@tauri-apps/api/core";

/** Mirrors `model::Risk` on the Rust side. */
export type Risk = "safe" | "rebuildable" | "caution";

export interface FileEntry {
  path: string;
  name: string;
  size: number;
  modified: number;
  ext: string;
}

export interface FolderEntry {
  path: string;
  name: string;
  size: number;
  fileCount: number;
}

export interface ScanProgress {
  phase: string;
  currentPath: string;
  entriesSeen: number;
  bytesSeen: number;
  roots: { path: string; bytesSeen: number; done: boolean }[];
}

export interface ScanResult {
  roots: VolumeInfo[];
  largestFiles: FileEntry[];
  largestFolders: FolderEntry[];
  totalBytes: number;
  totalFiles: number;
  unreadableDirs: number;
  elapsedMs: number;
  truncated: boolean;
}

export interface VolumeInfo {
  mount: string;
  label: string | null;
  fsType: string;
  totalBytes: number;
  availableBytes: number;
  usedBytes: number;
  removable: boolean;
}

export interface SystemInfo {
  osName: string;
  osVersion: string;
  home: string;
  separator: string;
}

export interface JunkTarget {
  id: string;
  label: string;
  category: string;
  path: string;
  size: number;
  entryCount: number;
  risk: Risk;
  needsElevation: boolean;
  /** Computed by Rust, so the default-selection policy has one source of truth. */
  selectedByDefault: boolean;
  reason: string;
}

export interface CategoryTotal {
  category: string;
  size: number;
  targetCount: number;
}

export interface CleanPlan {
  totalReclaimable: number;
  targetCount: number;
  requiresElevationCount: number;
  byCategory: CategoryTotal[];
  generatedAt: number;
}

export interface DetectProgress {
  phase: string;
  label: string;
  targetsFound: number;
  bytesSoFar: number;
}

export interface CleanOutcome {
  freedBytes: number;
  removedCount: number;
  skipped: { path: string; reason: string }[];
  failures: { path: string; reason: string }[];
}

/** Errors come back from Rust as `{ kind, message }`. */
export interface AppErrorShape {
  kind: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as AppErrorShape).message);
  }
  return "Something went wrong.";
}

export const systemInfo = () => invoke<SystemInfo>("system_info");

export const listVolumes = () => invoke<VolumeInfo[]>("list_volumes");

export const defaultScanRoots = () => invoke<string[]>("default_scan_roots");

export function startScan(
  roots: string[],
  onProgress: (p: ScanProgress) => void,
  options: { topFiles?: number; topFolders?: number; minFileSize?: number } = {},
): Promise<ScanResult> {
  return invoke<ScanResult>("start_scan", {
    roots,
    topFiles: options.topFiles ?? 500,
    topFolders: options.topFolders ?? 300,
    minFileSize: options.minFileSize ?? 0,
    onProgress: new Channel<ScanProgress>(onProgress),
  });
}

export const cancelScan = () => invoke<void>("cancel_scan");

export function detectJunk(
  includeStale: boolean,
  onProgress: (p: DetectProgress) => void,
): Promise<JunkTarget[]> {
  // The channel argument is always supplied: the Rust command declares it as a
  // required `Channel<T>`, since Tauri has no `Option<Channel<_>>` binding.
  return invoke<JunkTarget[]>("detect_junk", {
    includeStale,
    onProgress: new Channel<DetectProgress>(onProgress),
  });
}

export const previewClean = (ids: string[], includeStale = false) =>
  invoke<CleanPlan>("preview_clean", { ids, includeStale });

export const dryRunClean = (ids: string[], useTrash: boolean, includeStale = false) =>
  invoke<CleanOutcome>("dry_run_clean", {
    request: { ids, useTrash, allowPermanent: !useTrash },
    includeStale,
  });

export const runClean = (ids: string[], useTrash: boolean, includeStale = false) =>
  invoke<CleanOutcome>("clean", {
    request: { ids, useTrash, allowPermanent: !useTrash },
    includeStale,
  });