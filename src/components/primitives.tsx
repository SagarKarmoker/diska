import type { ReactNode } from "react";

import { formatBytes, percentOf } from "../lib/format";

export type Tone = "ok" | "warn" | "critical";

/** Pressure thresholds shared by every meter, so colours always agree. */
export function toneForPercent(pct: number): Tone {
  if (pct >= 90) return "critical";
  if (pct >= 75) return "warn";
  return "ok";
}

/**
 * Capacity meter. The fill width is a plain layout value that snaps to its new
 * size: there is no transition, by design.
 */
export function UsageBar({ used, total, tone }: { used: number; total: number; tone?: Tone }) {
  const pct = percentOf(used, total);
  const level = tone ?? toneForPercent(pct);

  return (
    <div
      className={`bar ${level === "critical" ? "is-critical" : level === "warn" ? "is-warn" : ""}`}
      role="meter"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`${pct.toFixed(1)} percent used`}
    >
      <div className="fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

export function RiskBadge({ risk }: { risk: "safe" | "rebuildable" | "caution" }) {
  const label = risk === "safe" ? "safe" : risk === "rebuildable" ? "rebuildable" : "review";
  return <span className={`risk ${risk}`}>{label}</span>;
}

/**
 * Section heading with a rule filling the remaining width. The rule replaces the
 * heavy borders an underlined heading would need.
 */
export function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="section">
      <h2>
        {title}
        {aside}
        <span className="rule" />
      </h2>
      {children}
    </section>
  );
}

/** A labelled figure. Used for the headline row of numbers. */
export function Stat({
  label,
  value,
  sub,
  accent,
}: {
  label: string;
  value: string;
  sub?: string;
  accent?: boolean;
}) {
  return (
    <div className="stat">
      <span className="k">{label}</span>
      <span className={`v ${accent ? "accent" : ""}`}>{value}</span>
      {sub && <span className="sub">{sub}</span>}
    </div>
  );
}

export function ErrorBox({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <div className="error" role="alert">
      {message}
    </div>
  );
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children}
    </div>
  );
}

/** Free space for the busiest volume, phrased as the actionable fact. */
export function Headroom({ free }: { free: number }) {
  return <span>{formatBytes(free)} free</span>;
}