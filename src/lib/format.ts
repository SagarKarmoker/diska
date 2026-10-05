const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/**
 * Human-readable byte size. Uses 1024-based units and keeps three significant
 * digits, so "1.23 GB" reads better than "1.23 GiB" for a disk tool.
 */
export function formatBytes(bytes: number, fractionDigits = 2): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes === 0) return "0 B";

  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }

  const digits = unit === 0 ? 0 : fractionDigits;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

/** Percentage of `total` taken by `part`, clamped to 0-100. */
export function percentOf(part: number, total: number): number {
  if (!total || total <= 0) return 0;
  return Math.max(0, Math.min(100, (part / total) * 100));
}

export function formatCount(n: number): string {
  return n.toLocaleString();
}

/** "3 minutes ago", used for file modification times. */
export function formatRelativeTime(millis: number): string {
  if (!millis) return "unknown";

  const seconds = Math.floor((Date.now() - millis) / 1000);
  if (seconds < 60) return "just now";

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;

  const days = Math.floor(hours / 24);
  if (days < 30) return `${days} day${days === 1 ? "" : "s"} ago`;

  const months = Math.floor(days / 30);
  if (months < 12) return `${months} month${months === 1 ? "" : "s"} ago`;

  const years = Math.floor(days / 365);
  return `${years} year${years === 1 ? "" : "s"} ago`;
}

/** Shorten a long absolute path for a fixed-width table cell. */
export function shortenPath(path: string, maxSegments = 5): string {
  const separator = path.includes("\\") ? "\\" : "/";
  const segments = path.split(separator).filter(Boolean);
  if (segments.length <= maxSegments) return path;

  return `${separator}…${separator}${segments.slice(-maxSegments).join(separator)}`;
}

/** The directory containing `path`, used to group files by folder in the UI. */
export function parentOf(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index <= 0 ? path : path.slice(0, index);
}