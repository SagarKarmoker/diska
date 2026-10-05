import type { ReactNode } from "react";
import { percentOf } from "../lib/format";

/**
 * Capacity bar. The fill width is a plain layout value: it snaps to the new
 * value rather than animating there.
 */
export function UsageBar({
  used,
  total,
  tone,
}: {
  used: number;
  total: number;
  tone?: "ok" | "warn" | "danger";
}) {
  const pct = percentOf(used, total);
  const level = tone ?? (pct >= 90 ? "danger" : pct >= 75 ? "warn" : "ok");

  return (
    <div
      className={`bar ${level === "danger" ? "is-danger" : level === "warn" ? "is-warn" : ""}`}
      role="meter"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`${pct.toFixed(1)} percent used`}
    >
      <div className={`fill ${level === "ok" ? "is-ok" : ""}`} style={{ width: `${pct}%` }} />
    </div>
  );
}

export function RiskBadge({ risk }: { risk: "safe" | "rebuildable" | "caution" }) {
  const label = risk === "safe" ? "safe" : risk === "rebuildable" ? "rebuildable" : "review";
  return <span className={`risk ${risk}`}>{label}</span>;
}

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="section">
      <h2>{title}</h2>
      {children}
    </section>
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

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>;
}