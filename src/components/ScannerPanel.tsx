import { useMemo, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { Empty, Section, Stat, UsageBar, toneForPercent } from "./primitives";
import {
  cancelScan,
  defaultScanRoots,
  errorMessage,
  startScan,
  type FileEntry,
  type FolderEntry,
  type ScanProgress,
  type ScanResult,
  type VolumeInfo,
} from "../lib/ipc";
import { formatBytes, formatCount, formatRelativeTime, percentOf, shortenPath } from "../lib/format";

type SortDir = "asc" | "desc";

/** Sorts numbers numerically and everything else as text. */
function compare(a: unknown, b: unknown, dir: SortDir): number {
  const sign = dir === "asc" ? 1 : -1;
  if (typeof a === "number" && typeof b === "number") return (a - b) * sign;
  return String(a).localeCompare(String(b)) * sign;
}

interface Column<T> {
  /** React key and column identity. */
  id: string;
  /** Field of `T` this column sorts by. Omit for presentational columns. */
  sort?: keyof T & string;
  /** Sort by this column on first render. Exactly one column should set it. */
  sortDefault?: boolean;
  label: string;
  className?: string;
  align?: "right";
  render: (row: T) => React.ReactNode;
}

/**
 * Sortable, filterable table.
 *
 * The default sort is whichever column is marked `sortDefault`, and the filter
 * runs before sorting so the order stays stable as the query narrows.
 */
function SortableTable<T>({
  rows,
  idField,
  columns,
  filter,
  placeholder,
  emptyTitle,
  emptyBody,
}: {
  rows: T[];
  idField: keyof T & string;
  columns: Column<T>[];
  filter: (row: T, needle: string) => boolean;
  placeholder: string;
  emptyTitle: string;
  emptyBody?: string;
}) {
  // Default to the column marked `sortDefault` (size, for both result tables),
  // not merely the first sortable column, which would sort by name.
  const defaultCol = columns.find((c) => c.sortDefault) ?? columns.find((c) => c.sort);
  const [sortKey, setSortKey] = useState<(keyof T & string) | undefined>(defaultCol?.sort);
  const [dir, setDir] = useState<SortDir>("desc");
  const [needle, setNeedle] = useState("");

  const visible = useMemo(() => {
    const q = needle.trim().toLowerCase();
    const matched = q ? rows.filter((r) => filter(r, q)) : rows;
    if (!sortKey) return matched;
    return [...matched].sort((a, b) => compare(a[sortKey], b[sortKey], dir));
  }, [rows, needle, sortKey, dir, filter]);

  const onHeader = (key: keyof T & string) => {
    if (key === sortKey) {
      setDir(dir === "asc" ? "desc" : "asc");
    } else {
      setSortKey(key);
      // Names read best A-Z, sizes best largest-first.
      setDir(key === "name" ? "asc" : "desc");
    }
  };

  return (
    <>
      <div className="toolbar">
        <input
          type="text"
          value={needle}
          placeholder={placeholder}
          onChange={(e) => setNeedle(e.target.value)}
          aria-label={placeholder}
        />
        <span className="tag">
          {formatCount(visible.length)} of {formatCount(rows.length)}
        </span>
        {needle && (
          <button className="btn ghost" onClick={() => setNeedle("")}>
            Clear
          </button>
        )}
      </div>

      {visible.length === 0 ? (
        <Empty title={emptyTitle}>{emptyBody}</Empty>
      ) : (
        <div className="tablewrap">
          <div className="tablescroll">
            <table>
              <thead>
                <tr>
                  {columns.map((c) => (
                    <th
                      key={c.id}
                      className={c.sort ? "sortable" : undefined}
                      style={c.align === "right" ? { textAlign: "right" } : undefined}
                      onClick={c.sort ? () => onHeader(c.sort as keyof T & string) : undefined}
                      aria-sort={
                        c.sort && sortKey === c.sort
                          ? dir === "asc"
                            ? "ascending"
                            : "descending"
                          : "none"
                      }
                    >
                      <span className="th">
                        {c.label}
                        {c.sort && sortKey === c.sort && (
                          <span className="caret">{dir === "asc" ? "▲" : "▼"}</span>
                        )}
                      </span>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {visible.map((row) => (
                  <tr key={String(row[idField])}>
                    {columns.map((c) => (
                      <td key={c.id} className={c.className}>
                        {c.render(row)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
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

      const res = await startScan(chosen, setProgress, { topFiles: 500, topFolders: 300 });
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
    // the item in its parent folder instead of handing the file to its default
    // application, so the UI can never launch anything on disk.
    revealItemInDir(path).catch(() => undefined);
  };

  // Bar widths are relative to the largest row, so the chart fills the space
  // regardless of absolute size. Computed once, not per row.
  const maxFile = result?.largestFiles.reduce((m, f) => Math.max(m, f.size), 0) ?? 0;
  const maxFolder = result?.largestFolders.reduce((m, f) => Math.max(m, f.size), 0) ?? 0;

const fileColumns: Column<FileEntry>[] = [
    {
      id: "name",
      sort: "name",
      label: "Name",
      className: "strong",
      render: (r) => (
        <button className="filelink" onClick={() => reveal(r.path)} title={r.path}>
          {r.name}
        </button>
      ),
    },
    {
      id: "size",
      sort: "size",
      sortDefault: true,
      label: "Size",
      className: "num",
      render: (r) => formatBytes(r.size),
    },
    {
      id: "bar",
      label: "",
      className: "barcell",
      render: (r) => <UsageBar used={r.size} total={maxFile} tone="ok" />,
    },
    {
      id: "modified",
      sort: "modified",
      label: "Modified",
      className: "num",
      render: (r) => formatRelativeTime(r.modified),
    },
    {
      id: "path",
      label: "Location",
      className: "path",
      render: (r) => shortenPath(r.path, 6),
    },
  ];

  const folderColumns: Column<FolderEntry>[] = [
    {
      id: "name",
      sort: "name",
      label: "Folder",
      className: "strong",
      render: (r) => (
        <button className="filelink" onClick={() => reveal(r.path)} title={r.path}>
          {r.name}
        </button>
      ),
    },
    {
      id: "size",
      sort: "size",
      sortDefault: true,
      label: "Size",
      className: "num",
      render: (r) => formatBytes(r.size),
    },
    {
      id: "bar",
      label: "",
      className: "barcell",
      render: (r) => <UsageBar used={r.size} total={maxFolder} tone="ok" />,
    },
    {
      id: "fileCount",
      sort: "fileCount",
      label: "Files",
      className: "num",
      render: (r) => formatCount(r.fileCount),
    },
    {
      id: "path",
      label: "Location",
      className: "path",
      render: (r) => shortenPath(r.path, 6),
    },
  ];

  return (
    <div className="panel">
      {error && <div className="error">{error}</div>}

      <Section title="Volumes">
        {volumes.length === 0 ? (
          <Empty title="No volumes reported">Mounted disks will appear here.</Empty>
        ) : (
          <div className="volumes">
            {volumes.map((v) => (
              <div className="volume" key={v.mount}>
                <div className="mount">
                  {v.mount}
                  {v.label && <span className="tag">{v.label}</span>}
                  <span className="tag">{v.fsType}</span>
                </div>
                <UsageBar used={v.usedBytes} total={v.totalBytes} />
                <div className="figures">
                  <span>
                    {formatBytes(v.usedBytes)} of {formatBytes(v.totalBytes)}
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
          Reads file metadata across{" "}
          {roots ? `${roots.length} location${roots.length === 1 ? "" : "s"}` : "the default locations"}{" "}
          and keeps only the largest entries, so memory stays flat no matter how full the disk
          is. File contents are never opened.
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
          <div className="meta">
            <span>{formatCount(progress.entriesSeen)} entries</span>
            <span className="sep" />
            <span>{formatBytes(progress.bytesSeen)} read</span>
            <span className="sep" />
            <span>{progress.currentPath}</span>
          </div>
        )}

        {result && (
          <>
            <div className="headline">
              <Stat label="Scanned" value={formatBytes(result.totalBytes)} />
              <Stat label="Files" value={formatCount(result.totalFiles)} />
              <Stat label="Took" value={`${(result.elapsedMs / 1000).toFixed(1)}s`} />
              {result.unreadableDirs > 0 && (
                <Stat
                  label="Skipped"
                  value={formatCount(result.unreadableDirs)}
                  sub="unreadable dirs"
                />
              )}
            </div>

            <Section title="Largest files">
              <SortableTable
                rows={result.largestFiles}
                idField="path"
                columns={fileColumns}
                filter={(r, q) => r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)}
                placeholder="Filter by name or path"
                emptyTitle="No files matched"
                emptyBody="Try a shorter search."
              />
            </Section>

            <Section title="Largest folders">
              <SortableTable
                rows={result.largestFolders}
                idField="path"
                columns={folderColumns}
                filter={(r, q) => r.name.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)}
                placeholder="Filter by folder name or path"
                emptyTitle="No folders matched"
                emptyBody="Try a shorter search."
              />
            </Section>
          </>
        )}
      </Section>
    </div>
  );
}

/** Shared pressure reading used by the app bar and the overview. */
export function pressureOf(volumes: VolumeInfo[]): VolumeInfo | null {
  if (volumes.length === 0) return null;
  return volumes.reduce((worst, v) =>
    percentOf(v.usedBytes, v.totalBytes) > percentOf(worst.usedBytes, worst.totalBytes) ? v : worst,
  );
}

export { toneForPercent };