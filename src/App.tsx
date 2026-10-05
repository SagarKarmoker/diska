import { useEffect, useState } from "react";

import { CleanerPanel } from "./components/CleanerPanel";
import { ScannerPanel } from "./components/ScannerPanel";
import { Empty, Section, UsageBar } from "./components/primitives";
import {
  errorMessage,
  listVolumes,
  systemInfo,
  type SystemInfo,
  type VolumeInfo,
} from "./lib/ipc";
import { formatBytes, percentOf } from "./lib/format";

type Tab = "overview" | "largest" | "clean";

const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "largest", label: "Largest files" },
  { id: "clean", label: "Clean" },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("overview");
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Volume and OS lookups are independent, so neither has to wait for the
    // other before the UI renders.
    systemInfo()
      .then(setInfo)
      .catch((e) => setError(errorMessage(e)));
    listVolumes()
      .then(setVolumes)
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const busiest = volumes.reduce<VolumeInfo | null>(
    (worst, v) => (worst === null || percentOf(v.usedBytes, v.totalBytes) > percentOf(worst.usedBytes, worst.totalBytes) ? v : worst),
    null,
  );

  return (
    <div className="app">
      <header className="titlebar">
        <h1>diska</h1>
        {info && (
          <span className="os">
            {info.osName} {info.osVersion} · {info.home}
          </span>
        )}
      </header>

      <nav className="tabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            className="tab"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      {error && (
        <div className="panel">
          <div className="error">{error}</div>
        </div>
      )}

      {tab === "overview" && (
        <div className="panel">
          <Section title="Where your space went">
            <p className="note">
              Run a scan to find the files and folders eating your disk. Nothing is deleted here.
            </p>
            <div className="toolbar">
              <button className="btn primary" onClick={() => setTab("largest")}>
                Scan for largest files
              </button>
              <button className="btn" onClick={() => setTab("clean")}>
                Clean caches
              </button>
            </div>
          </Section>

          <Section title="Capacity">
            {volumes.length === 0 ? (
              <Empty>No fixed volumes reported.</Empty>
            ) : (
              <div className="volumes">
                {volumes.map((v) => (
                  <div className="volume" key={v.mount}>
                    <div className="mount">
                      {v.mount}
                      {v.label ? <span className="tag">{v.label}</span> : null}
                      <span className="tag">{v.fsType}</span>
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
        </div>
      )}

      {tab === "largest" && <ScannerPanel volumes={volumes} onVolumes={setVolumes} />}
      {tab === "clean" && <CleanerPanel />}

      <footer className="statusbar">
        {busiest ? (
          <>
            <span>
              {formatBytes(busiest.usedBytes)} used of {formatBytes(busiest.totalBytes)}
            </span>
            <span>{formatBytes(busiest.availableBytes)} free</span>
            <span className="spacer" />
            <span>
              {percentOf(busiest.usedBytes, busiest.totalBytes).toFixed(1)}% full on{" "}
              {busiest.mount}
            </span>
          </>
        ) : (
          <>
            <span>No volume information yet</span>
            <span className="spacer" />
          </>
        )}
      </footer>
    </div>
  );
}