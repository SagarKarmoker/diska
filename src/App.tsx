import { useEffect, useState } from "react";

import { AboutPanel } from "./components/AboutPanel";
import { CleanerPanel } from "./components/CleanerPanel";
import { ScannerPanel, pressureOf } from "./components/ScannerPanel";
import { Empty, Section, Stat, UsageBar, toneForPercent } from "./components/primitives";
import { errorMessage, listVolumes, systemInfo, type SystemInfo, type VolumeInfo } from "./lib/ipc";
import { formatBytes, percentOf } from "./lib/format";

type View = "overview" | "largest" | "clean" | "about";

const NAV: { id: View; label: string; hint: string }[] = [
  { id: "overview", label: "Overview", hint: "Capacity at a glance" },
  { id: "largest", label: "Largest files", hint: "What is using space" },
  { id: "clean", label: "Clean", hint: "Reclaim caches" },
  { id: "about", label: "About", hint: "Version and credits" },
];

/** Minimal inline glyphs. Drawn rather than iconified so there is no dependency. */
function Glyph({ id }: { id: View }) {
  const common = {
    width: 15,
    height: 15,
    viewBox: "0 0 16 16",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.5,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (id === "overview")
    return (
      <svg {...common}>
        <rect x="2" y="2.5" width="12" height="11" rx="1.5" />
        <path d="M2 10h3M13 10h1M5 13.5v-1M8 13.5v-1M11 13.5v-1" />
      </svg>
    );
  if (id === "largest")
    return (
      <svg {...common}>
        <path d="M2 4h12M2 8h8M2 12h5" />
      </svg>
    );
  if (id === "clean")
    return (
      <svg {...common}>
        <path d="M2.5 4h11l-.8 8.2a1 1 0 0 1-1 .9H4.3a1 1 0 0 1-1-.9L2.5 4Z" />
        <path d="M6 4V2.8M10 4V2.8M6.2 7.5v2.8M9.8 7.5v2.8" />
      </svg>
    );
  return (
    <svg {...common}>
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7.2v4M8 4.9v.6" />
    </svg>
  );
}

export default function App() {
  const [view, setView] = useState<View>("overview");
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Independent lookups: neither has to wait for the other to render.
    systemInfo()
      .then(setInfo)
      .catch((e) => setError(errorMessage(e)));
    listVolumes()
      .then(setVolumes)
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const busiest = pressureOf(volumes);
  const usedPct = busiest ? percentOf(busiest.usedBytes, busiest.totalBytes) : 0;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="wordmark">
            diska
            <span className="sub">1.0</span>
          </span>
        </div>

        <nav className="nav" aria-label="Sections">
          {NAV.map((item) => (
            <button
              key={item.id}
              className="navitem"
              aria-current={view === item.id ? "page" : undefined}
              // The label text is hidden when the sidebar collapses to a rail,
              // so the accessible name is set explicitly rather than being
              // derived from the visible text.
              aria-label={item.label}
              title={item.label}
              onClick={() => setView(item.id)}
            >
              <Glyph id={item.id} />
              <span className="navtext" aria-hidden="true">
                <span className="navlabel">{item.label}</span>
                <span className="navhint">{item.hint}</span>
              </span>
            </button>
          ))}
        </nav>

        <div className="sidefoot">
          <div className="sidefoot-head">
            <span className="k">Tightest volume</span>
            <span className="pct">{usedPct.toFixed(0)}%</span>
          </div>
          {busiest ? (
            <>
              <UsageBar used={busiest.usedBytes} total={busiest.totalBytes} />
              <div className="sidefoot-figures">
                <span className="mono">{formatBytes(busiest.availableBytes)} free</span>
                <span className="tag">{busiest.fsType}</span>
              </div>
            </>
          ) : (
            <p className="sidefoot-empty">No volume data yet.</p>
          )}

          {info && (
            <p className="sidefoot-os">
              {info.osName} {info.osVersion}
            </p>
          )}
        </div>
      </aside>

      <main className="main">
        {error && (
          <div className="panel">
            <div className="error">{error}</div>
          </div>
        )}

        {view === "overview" && (
          <div className="panel">
            <Section title="Your disk">
              <div className="headline">
                {busiest ? (
                  <>
                    <Stat
                      label="Free"
                      value={formatBytes(busiest.availableBytes)}
                      sub={`of ${formatBytes(busiest.totalBytes)} on ${busiest.mount}`}
                      accent
                    />
                    <Stat
                      label="Used"
                      value={`${usedPct.toFixed(1)}%`}
                      sub={formatBytes(busiest.usedBytes)}
                    />
                    <Stat
                      label="Pressure"
                      value={
                        toneForPercent(usedPct) === "critical"
                          ? "Critical"
                          : toneForPercent(usedPct) === "warn"
                            ? "Getting tight"
                            : "Healthy"
                      }
                      sub={
                        toneForPercent(usedPct) === "critical"
                          ? "Under 10% remaining"
                          : toneForPercent(usedPct) === "warn"
                            ? "Consider a cleanup"
                            : "Room to spare"
                      }
                    />
                  </>
                ) : (
                  <Stat label="Status" value="Reading volumes…" />
                )}
              </div>

              <div className="toolbar">
                <button className="btn primary" onClick={() => setView("largest")}>
                  Find what is using space
                </button>
                <button className="btn" onClick={() => setView("clean")}>
                  Reclaim caches
                </button>
              </div>
            </Section>

            <Section title="Capacity">
              {volumes.length === 0 ? (
                <Empty title="No volumes reported">
                  Fixed disks will appear here once detected.
                </Empty>
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
          </div>
        )}

        {view === "largest" && <ScannerPanel volumes={volumes} onVolumes={setVolumes} />}
        {view === "clean" && <CleanerPanel />}
        {view === "about" && <AboutPanel />}
      </main>

      <footer className="statusbar">
        {busiest ? (
          <>
            <span>{busiest.mount}</span>
            <span className="sep" />
            <span>
              {formatBytes(busiest.usedBytes)} used of {formatBytes(busiest.totalBytes)}
            </span>
            <span className="spacer" />
            <span>
              {formatBytes(busiest.availableBytes)} free · {usedPct.toFixed(1)}% full
            </span>
          </>
        ) : (
          <>
            <span>Waiting for volume information</span>
            <span className="spacer" />
          </>
        )}
      </footer>
    </div>
  );
}