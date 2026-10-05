import { openUrl } from "@tauri-apps/plugin-opener";

import { Section, Stat, ErrorBox } from "./primitives";
import { buildInfo, errorMessage, type BuildInfo } from "../lib/ipc";
import { useEffect, useState } from "react";

const REPO = "https://github.com/SagarKarmoker/diska";
const ISSUES = "https://github.com/SagarKarmoker/diska/issues";
const AUTHOR = { name: "Sagar Karmoker", handle: "SagarKarmoker" };

const DEPENDENCIES = [
  { name: "Tauri", version: "2", role: "app shell and IPC" },
  { name: "Rust", role: "scan, detection and deletion" },
  { name: "React", role: "interface" },
  { name: "ignore", role: "parallel filesystem walk" },
  { name: "sysinfo", role: "volume and OS information" },
  { name: "trash", role: "moving files to the system trash" },
  { name: "IBM Plex", role: "typeface" },
];

/** Split into two near-equal columns, longer column first. */
function splitEvenly<T>(items: T[]): [T[], T[]] {
  const half = Math.ceil(items.length / 2);
  return [items.slice(0, half), items.slice(half)];
}

export function AboutPanel() {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    buildInfo()
      .then(setInfo)
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const open = (url: string) => {
    // Only ever a fixed github.com URL from the constants above, never anything
    // derived from filesystem or app data.
    openUrl(url).catch(() => undefined);
  };

  return (
    <div className="panel">
      <ErrorBox message={error} />

      <Section title="diska">
        <div className="headline">
          <Stat label="Version" value={info?.version ?? "—"} />
          <Stat label="Licence" value="MIT" />
          <Stat label="Platform" value={info ? `${info.osName} ${info.arch}` : "—"} />
        </div>

        <p className="note">
          A cross-platform disk usage analyzer and cache cleaner. It finds what is filling a
          disk, lists the largest files and folders, and reclaims regenerable caches behind a
          confirmation that names every path it will touch.
        </p>
      </Section>

      <Section title="Developer">
        <div className="cardrow">
          <div className="card">
            <span className="cardlabel">Author</span>
            <span className="cardvalue">{AUTHOR.name}</span>
            <button className="btn" onClick={() => open(`https://github.com/${AUTHOR.handle}`)}>
              @{AUTHOR.handle}
            </button>
          </div>

          <div className="card">
            <span className="cardlabel">Source</span>
            <span className="cardvalue mono">{REPO.replace("https://github.com/", "")}</span>
            <div className="cardactions">
              <button className="btn primary" onClick={() => open(REPO)}>
                View repository
              </button>
              <button className="btn" onClick={() => open(ISSUES)}>
                Report an issue
              </button>
            </div>
          </div>
        </div>

        <p className="note">
          Bug reports and pull requests are welcome. If the cleaner refuses to remove something
          you expected, include what the target was and what it said &mdash; the refusal reasons
          are deliberately explicit.
        </p>
      </Section>

      <Section title="Build">
        <dl className="facts">
          <div className="factcol">
            <div className="factrow">
              <dt>Application</dt>
              <dd className="mono">{info ? `${info.appName} ${info.version}` : "—"}</dd>
            </div>
            <div className="factrow">
              <dt>Operating system</dt>
              <dd className="mono">{info ? `${info.osName} ${info.osVersion}` : "—"}</dd>
            </div>
            <div className="factrow">
              <dt>Kernel</dt>
              <dd className="mono">{info?.kernel ?? "—"}</dd>
            </div>
            <div className="factrow">
              <dt>Architecture</dt>
              <dd className="mono">{info?.arch ?? "—"}</dd>
            </div>
          </div>
          <div className="factcol">
            <div className="factrow">
              <dt>Tauri</dt>
              <dd className="mono">{info?.tauriVersion ?? "—"}</dd>
            </div>
            <div className="factrow">
              <dt>Rust toolchain</dt>
              <dd className="mono">{info?.rustVersion ?? "—"}</dd>
            </div>
            {info?.packageManager && (
              <div className="factrow">
                <dt>Package manager</dt>
                <dd className="mono">{info.packageManager}</dd>
              </div>
            )}
          </div>
        </dl>
      </Section>

      <Section title="Built with">
        <dl className="facts">
          {[0, 1].map((column) => (
            <div className="factcol" key={column}>
              {splitEvenly(DEPENDENCIES)[column].map((d) => (
                <div className="factrow" key={d.name}>
                  <dt>{d.name}</dt>
                  <dd>
                    {d.version && <span className="mono">{d.version}</span>}
                    {d.version && " · "}
                    {d.role}
                  </dd>
                </div>
              ))}
            </div>
          ))}
        </dl>
      </Section>

      <Section title="Safety">
        <ul className="bullets">
          <li>
            Files move to the <strong>system trash</strong> by default. Permanent deletion has to be
            asked for separately, in a dialog that spells out that it cannot be undone.
          </li>
          <li>
            Clean requests carry rule <strong>ids, not paths</strong>. The backend re-runs detection
            and resolves them itself, so a request cannot name an arbitrary file.
          </li>
          <li>
            Nothing outside your user profile is ever deleted. Items needing administrator rights
            are reported and skipped &mdash; the app never escalates.
          </li>
          <li>
            Symlinks are <strong>unlinked, never followed</strong>, so a link cannot be used to
            delete data the link itself did not own.
          </li>
        </ul>
      </Section>
    </div>
  );
}