import { useMemo, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { Section, UsageBar, Empty } from "./primitives";
import {
  cancelScan,
  defaultScanRoots,
  errorMessage,
  startScan,
  type FolderEntry,
  type FileEntry,
  type ScanProgress,
  type ScanResult,
  type VolumeInfo,
} from "../lib/ipc";
import {
  formatBytes,
  formatCount,
  formatRelativeTime,
  percentOf,
  shortenPath,
} from "../lib/format";

type SortDir = "asc" | "desc";

function comparator<T>(a: T, b: T, key: keyof T & string, dir: SortDir): number {
  const sign = dir === "asc" ? 1 : -1;
  const left = a[key];
  const right = b[key];

  if (typeof left === "number" && typeof right === "number") {
    return (left - right) * sign;
  }
  return String(left).localeCompare(String(right)) * sign;
}

interface Column<T> {
  /** React key. Presentational columns use an arbitrary string like "bar". */
  id: string;
  /** Field of `T` this column sorts by. Omit for presentational columns. */
  sort?: keyof T & string;
  label: string;
  className?: string;
  render: (row: T) => React.ReactNode;
}

/** Sortable, filterable table for the scan results. */
function SortableTable<T>({
  rows,
  keyField,
  columns,
  filter,
  filterPlaceholder,
  emptyMessage,
}: {
  rows: T[];
  keyField: keyof T & string;
  columns: Column<T>[];
  filter: (row: T, needle: string) => boolean;
  filterPlaceholder: string;
  emptyMessage: string;
}) {
  const firstSortKey = (columns.find((c) => c.sort)?.sort ?? "size") as keyof T & string;
  const [sortKey, setSortKey] = useState<keyof T & string>(firstSortKey);
  const [dir, setDir] = useState<SortDir>("desc");
  const [needle, setNeedle] = useState("");

  const visible = useMemo(() => {
    const q = needle.trim().toLowerCase();
    const filtered = q ? rows.filter((r) => filter(r, q)) : rows;
    return [...filtered].sort((a, b) => comparator(a, b, sortKey, dir));
  }, [rows, needle, sortKey, dir, filter]);

  const onHeader = (key: NonNullable<keyof T & string>) => {
    if (key === sortKey) {
      setDir(dir === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      setDir(key === "name" ? "asc" : "desc");
    }
  };

  return (
    <>
      <div className="toolbar">
        <input
          type="text"
          value={needle}
          placeholder={filterPlaceholder}
          onChange={(e) => setNeedle(e.target.value)}
          aria-label={filterPlaceholder}
        />
        <span className="tag">
          {formatCount(visible.length)} of {formatCount(rows.length)} shown
        </span>
      </div>

      {visible.length === 0 ? (
        <Empty>{emptyMessage}</Empty>
      ) : (
        <table>
          <thead>
            <tr>
              {columns.map((c) => (
                <th
                  key={c.id}
                  className={c.sort ? undefined : "static"}
                  onClick={c.sort ? () => onHeader(c.sort as keyof T & string) : undefined}
                  aria-sort={
                    c.sort && sortKey === c.sort
                      ? dir === "asc"
                        ? "ascending"
                        : "descending"
                      : "none"
                  }
                >
                  {c.label}
                  {c.sort && sortKey === c.sort ? (dir === "asc" ? " \u25b2" : " \u25bc") : ""}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.map((row) => (
              <tr key={String(row[keyField])}>
                {columns.map((c) => (
                  <td key={c.id} className={c.className}>
                    {c.render(row)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}

export function ScannerPanel({
  volumes,
  onVolumes,
}: {
  volumes: VolumeInfo[];
  onVolumes: (v: VolumeInfo[]) => void;
}) {
  const [roots, setRoots] = useState<string[] | null>(null);
  const [result, setResult] = useState<ScanResult | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scan = async () => {
    setBusy(true);
    setError(null);
    setResult(null);
    setProgress(null);

    try {
      const chosen = roots ?? (await defaultScanRoots());
      if (!roots) setRoots(chosen);

      const res = await startScan(chosen, setProgress, {
        topFiles: 500,
        topFolders: 300,
      });

      setResult(res);
      onVolumes(res.roots);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const reveal = (path: string) => {
    // Revealing in the file manager is deliberate over `openPath`: it selects
    // the item in the parent folder instead of handing the file to its default
    // application, so the UI can never launch anything on disk. It also stays
    // within the permissions the opener plugin grants by default.
    revealItemInDir(path).catch(() => undefined);
  };

  // Bar widths are relative to the largest row so the chart always fills the
  // available space regardless of the absolute sizes. Computed once per render
  // rather than per row.
  const maxFileSize = result?.largestFiles.reduce((m, f) => Math.max(m, f.size), 0) ?? 0;
  const maxFolderSize = result?.largestFolders.reduce((m, f) => Math.max(m, f.size), 0) ?? 0;

  const fileColumns: Column<FileEntry>[] = [
    {
      id: "name",
      sort: "name",
      label: "Name",
      className: "name",
      render: (r: FileEntry) => (
        <button className="btn" onClick={() => reveal(r.path)} title={r.path}>
          {r.name}
        </button>
      ),
    },
    {
      id: "size",
      sort: "size",
      label: "Size",
      className: "num",
      render: (r: FileEntry) => formatBytes(r.size),
    },
    {
      id: "modified",
      sort: "modified",
      label: "Modified",
      className: "num",
      render: (r: FileEntry) => formatRelativeTime(r.modified),
    },
    {
      id: "bar",
      label: "",
      className: "bar-cell",
      render: (r: FileEntry) => (
        <div className="bar">
          <div className="fill is-ok" style={{ width: `${percentOf(r.size, maxFileSize)}%` }} />
        </div>
      ),
    },
    {
      id: "path",
      label: "Path",
      className: "path",
      render: (r: FileEntry) => shortenPath(r.path, 6),
    },
  ];

  const folderColumns: Column<FolderEntry>[] = [
    {
      id: "name",
      sort: "name",
      label: "Folder",
      className: "name",
      render: (r: FolderEntry) => (
        <button className="btn" onClick={() => reveal(r.path)} title={r.path}>
          {r.name}
        </button>
      ),
    },
    {
      id: "size",
      sort: "size",
      label: "Size",
      className: "num",
      render: (r: FolderEntry) => formatBytes(r.size),
    },
    {
      id: "fileCount",
      sort: "fileCount",
      label: "Files",
      className: "num",
      render: (r: FolderEntry) => formatCount(r.fileCount),
    },
    {
      id: "bar",
      label: "",
      className: "bar-cell",
      render: (r: FolderEntry) => (
        <div className="bar">
          <div className="fill is-ok" style={{ width: `${percentOf(r.size, maxFolderSize)}%` }} />
        </div>
      ),
    },
    {
      id: "path",
      label: "Path",
      className: "path",
      render: (r: FolderEntry) => shortenPath(r.path, 6),
    },
  ];

  return (
    <div className="panel">
      {error && <div className="error">{error}</div>}

      <Section title="Volumes">
        {volumes.length === 0 ? (
          <Empty>No fixed volumes reported yet.</Empty>
        ) : (
          <div className="volumes">
            {volumes.map((v) => (
              <div className="volume" key={v.mount}>
                <div className="mount">
                  {v.mount}
                  {v.label ? <span className="tag">{v.label}</span> : null}
                  <span className="tag">{v.fsType}</span>
                  {v.removable ? <span className="tag">removable</span> : null}
                </div>
                <UsageBar used={v.usedBytes} total={v.totalBytes} />
                <div className="figures">
                  <span>
                    {formatBytes(v.usedBytes)} used of {formatBytes(v.totalBytes)}
                  </span>
                  <span>{formatBytes(v.availableBytes)} free</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </Section>

      <Section title="Scan">
        <p className="note">
          Walks {roots ? `${roots.length} location(s)` : "the default locations"} and keeps only
          the largest files and folders, so memory stays flat no matter how many files the disk
          holds. This reads metadata only and never opens file contents.
        </p>
        <div className="toolbar">
          <button className="btn primary" onClick={scan} disabled={busy}>
            {busy ? "Scanning…" : result ? "Rescan" : "Start scan"}
          </button>
          {busy && (
            <button className="btn" onClick={() => cancelScan().catch(() => undefined)}>
              Stop
            </button>
          )}
          {roots && <span className="tag">{roots.join("  ")}</span>}
        </div>

        {busy && progress && (
          <div className="results">
            <span>
              {formatCount(progress.entriesSeen)} entries
            </span>
            <span>{formatBytes(progress.bytesSeen)} read</span>
            <span>{progress.currentPath}</span>
          </div>
        )}

        {result && (
          <>
            <div className="results">
              <span>{formatBytes(result.totalBytes)} total</span>
              <span>{formatCount(result.totalFiles)} files</span>
              <span>{(result.elapsedMs / 1000).toFixed(1)}s</span>
              {result.unreadableDirs > 0 && (
                <span>{formatCount(result.unreadableDirs)} unreadable dirs skipped</span>
              )}
              {result.truncated && <span>top-N only</span>}
            </div>

            <Section title="Largest files">
              <SortableTable
                rows={result.largestFiles}
                keyField="path"
                columns={fileColumns}
                filterPlaceholder="Filter by name or path"
                emptyMessage="No files matched."
                filter={(r, q) =>
                  r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)
                }
              />
            </Section>

            <Section title="Largest folders">
              <SortableTable
                rows={result.largestFolders}
                keyField="path"
                columns={folderColumns}
                filterPlaceholder="Filter by folder name or path"
                emptyMessage="No folders matched."
                filter={(r, q) =>
                  r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)
                }
              />
            </Section>
          </>
        )}
      </Section>
    </div>
  );
}