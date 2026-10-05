import { useMemo, useState } from "react";

import { Empty, RiskBadge, Section } from "./primitives";
import {
  detectJunk,
  dryRunClean,
  errorMessage,
  previewClean,
  runClean,
  type CleanOutcome,
  type CleanPlan,
  type DetectProgress,
  type JunkTarget,
} from "../lib/ipc";
import { formatBytes, formatCount } from "../lib/format";

const RISK_ORDER = { safe: 0, rebuildable: 1, caution: 2 } as const;

/**
 * Confirmation dialog.
 *
 * Deleting is irreversible, so the dialog states the exact paths and byte
 * totals, and separates "moves to trash" from "gone for good". Every reason
 * string from the backend is repeated here so the decision is made with the
 * same information the rule was defined with.
 */
function ConfirmDialog({
  plan,
  targets,
  useTrash,
  busy,
  onCancel,
  onConfirm,
}: {
  plan: CleanPlan;
  targets: JunkTarget[];
  useTrash: boolean;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const elevated = targets.filter((t) => t.needsElevation);
  const cautious = targets.filter((t) => t.risk === "caution");

  return (
    <div className="backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-label="Confirm clean">
        <h2>{useTrash ? "Move these to the trash?" : "Delete these permanently?"}</h2>
        <p className="note">
          {useTrash
            ? "Everything moves to your system trash, so you can restore anything you did not mean to select."
            : "These files are removed immediately. There is no trash and no undo."}
        </p>

        <div className="totals">
          <div>
            <span className="k">Space freed</span>
            <span className="v">{formatBytes(plan.totalReclaimable)}</span>
          </div>
          <div>
            <span className="k">Locations</span>
            <span className="v">{formatCount(plan.targetCount)}</span>
          </div>
          <div>
            <span className="k">Items</span>
            <span className="v">{formatCount(plan.totalReclaimable > 0 ? targets.length : 0)}</span>
          </div>
        </div>

        {!useTrash && (
          <div className="callout danger">
            <span>
              <b>Permanent deletion.</b> This cannot be undone. If you are unsure, use the trash
              instead.
            </span>
          </div>
        )}

        {cautious.length > 0 && (
          <div className="callout">
            <span>
              <b>
                {cautious.length} of these may hold real data
              </b>{" "}
              rather than regenerable cache:{" "}
              {cautious.map((t) => t.label).join(", ")}.
            </span>
          </div>
        )}

        {elevated.length > 0 && (
          <div className="callout">
            <span>
              <b>
                {elevated.length} location{elevated.length === 1 ? "" : "s"} need administrator
                rights
              </b>{" "}
              and will be skipped. diska never asks for elevated permissions.
            </span>
          </div>
        )}

        <ul className="paths">
          {targets.map((t) => (
            <li key={t.id}>
              {formatBytes(t.size).padStart(10)}  {t.path}
            </li>
          ))}
        </ul>

        <div className="actions">
          <button className="btn" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button
            className={useTrash ? "btn primary" : "btn danger"}
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? "Working…" : useTrash ? "Move to trash" : "Delete permanently"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Outcome({ outcome }: { outcome: CleanOutcome }) {
  return (
    <>
      <div className="meta">
        <span>Freed {formatBytes(outcome.freedBytes)}</span>
        <span className="sep" />
        <span>{formatCount(outcome.removedCount)} items removed</span>
        {outcome.skipped.length > 0 && (
          <>
            <span className="sep" />
            <span>{formatCount(outcome.skipped.length)} skipped</span>
          </>
        )}
        {outcome.failures.length > 0 && (
          <>
            <span className="sep" />
            <span>{formatCount(outcome.failures.length)} failed</span>
          </>
        )}
      </div>

      {(outcome.skipped.length > 0 || outcome.failures.length > 0) && (
        <details>
          <summary>Why some files were left alone</summary>
          <ul className="paths">
            {outcome.failures.map((f) => (
              <li className="bad" key={`f-${f.path}`}>
                {f.path} — {f.reason}
              </li>
            ))}
            {outcome.skipped.map((s) => (
              <li key={`s-${s.path}`}>
                {s.path} — {s.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}

export function CleanerPanel() {
  const [includeStale, setIncludeStale] = useState(false);
  const [targets, setTargets] = useState<JunkTarget[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<DetectProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [plan, setPlan] = useState<CleanPlan | null>(null);
  const [useTrash, setUseTrash] = useState(true);
  const [outcome, setOutcome] = useState<CleanOutcome | null>(null);

  /**
   * Ticks whatever Rust marked as safe to select. The policy lives in
   * `junk::detect` next to the rules that assign risk, so the UI cannot drift
   * out of step with the risk tiers.
   */
  const initialSelection = (found: JunkTarget[]) =>
    new Set(found.filter((t) => t.selectedByDefault).map((t) => t.id));

  const detect = async () => {
    setBusy(true);
    setError(null);
    setOutcome(null);
    setProgress(null);

    try {
      const found = await detectJunk(includeStale, setProgress);
      setTargets(found);
      setSelected(initialSelection(found));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const grouped = useMemo(() => {
    if (!targets) return [];

    const map = new Map<string, JunkTarget[]>();
    for (const t of targets) {
      const list = map.get(t.category) ?? [];
      list.push(t);
      map.set(t.category, list);
    }

    return [...map.entries()]
      .map(([category, items]) => ({
        category,
        items: [...items].sort(
          (a, b) => RISK_ORDER[a.risk] - RISK_ORDER[b.risk] || b.size - a.size,
        ),
        size: items.reduce((n, t) => n + t.size, 0),
      }))
      .sort((a, b) => b.size - a.size);
  }, [targets]);

  const selectedTargets = useMemo(
    () => targets?.filter((t) => selected.has(t.id)) ?? [],
    [targets, selected],
  );
  const selectedBytes = selectedTargets.reduce((n, t) => n + t.size, 0);

  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const setMany = (ids: string[], on: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });

  const askToClean = async () => {
    setBusy(true);
    setError(null);
    try {
      // Dry run first, so the dialog quotes the same code path that will run.
      // Its outcome is intentionally not displayed: nothing has been removed
      // yet, and reporting "items removed" before confirmation would be a lie.
      await dryRunClean([...selected], useTrash, includeStale);
      setPlan(await previewClean([...selected], includeStale));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const confirmClean = async () => {
    setBusy(true);
    setError(null);
    try {
      setOutcome(await runClean([...selected], useTrash, includeStale));
      setPlan(null);
      // Sizes are stale after a clean; re-detect so the list reflects reality.
      const found = await detectJunk(includeStale, () => undefined);
      setTargets(found);
      setSelected(initialSelection(found));
    } catch (e) {
      setError(errorMessage(e));
      setPlan(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}

      <Section title="Reclaim space">
        <p className="note">
          Every location below is a well-known cache. Only fully regenerable items are
          pre-selected, and nothing is removed without a confirmation that lists each path.
        </p>

        <div className="toolbar">
          <button className="btn primary" onClick={detect} disabled={busy}>
            {busy ? "Detecting…" : targets ? "Rescan" : "Find junk"}
          </button>
          {targets && (
            <span className="toolbar-note">
              {formatCount(targets.length)} location{targets.length === 1 ? "" : "s"} found
            </span>
          )}
        </div>

        {/* Options are a labelled group rather than loose controls in the
            toolbar: the primary action stays alone, and the two settings read
            as settings with their consequences spelled out. */}
        <div className="settings">
          <div className="setting">
            <label className="setting-text" htmlFor="opt-sweep">
              <span className="setting-name">Aggressive sweep</span>
              <span className="setting-hint">
                Also looks for old installers, stale logs and build output. Slower, and may
                include files you still want.
              </span>
            </label>
            <input
              id="opt-sweep"
              className="switch"
              type="checkbox"
              role="switch"
              checked={includeStale}
              onChange={(e) => setIncludeStale(e.target.checked)}
            />
          </div>

          <div className="setting">
            <span className="setting-text">
              <span className="setting-name">Where files go</span>
              <span className="setting-hint">
                {useTrash
                  ? "Moved to your system trash, so anything can be restored."
                  : "Deleted immediately with no way to recover them."}
              </span>
            </span>
            {/* Trash vs permanent is a choice between two modes, not an
                on/off preference, so it is a segmented control rather than a
                checkbox whose off state is easy to misread. */}
            <div className="segmented" role="radiogroup" aria-label="Where files go">
              <button
                type="button"
                role="radio"
                aria-checked={useTrash}
                className={useTrash ? "on" : undefined}
                onClick={() => setUseTrash(true)}
              >
                Trash
              </button>
              <button
                type="button"
                role="radio"
                aria-checked={!useTrash}
                className={!useTrash ? "on is-danger" : undefined}
                onClick={() => setUseTrash(false)}
              >
                Permanent
              </button>
            </div>
          </div>
        </div>

        {busy && progress && (
          <div className="meta">
            <span>{progress.label}</span>
            <span className="sep" />
            <span>
              {formatCount(progress.targetsFound)} found · {formatBytes(progress.bytesSoFar)}
            </span>
          </div>
        )}

        {outcome && <Outcome outcome={outcome} />}
      </Section>

      {targets && targets.length === 0 && (
        <Empty title="Nothing to clean">
          Your caches are already tidy. Turn on the aggressive sweep to look for old installers,
          stale logs and build output as well.
        </Empty>
      )}

      {grouped.map(({ category, items, size }) => {
        const ids = items.map((t) => t.id);
        const allOn = ids.every((id) => selected.has(id));
        const someOn = ids.some((id) => selected.has(id));

        return (
          <div className="category" key={category}>
            <header>
              <label className="check">
                <input
                  type="checkbox"
                  checked={allOn}
                  ref={(el) => {
                    if (el) el.indeterminate = !allOn && someOn;
                  }}
                  onChange={(e) => setMany(ids, e.target.checked)}
                />
                <span className="name">{category}</span>
              </label>
              <span className="figures">
                {formatBytes(size)} · {items.length} item{items.length === 1 ? "" : "s"}
              </span>
            </header>

            {items.map((t) => {
              const on = selected.has(t.id);
              return (
                <label className={`target ${on ? "" : "is-off"}`} key={t.id}>
                  <input
                    type="checkbox"
                    checked={on}
                    disabled={t.needsElevation}
                    onChange={() => toggle(t.id)}
                  />
                  <span className="body">
                    <span className="label">
                      <span>{t.label}</span>
                      <RiskBadge risk={t.risk} />
                      {t.needsElevation && <span className="tag">needs admin</span>}
                    </span>
                    <span className="reason">{t.reason}</span>
                    <span className="where">
                      {t.path} · {formatCount(t.entryCount)} entries
                    </span>
                  </span>
                  <span className="size">{formatBytes(t.size)}</span>
                </label>
              );
            })}
          </div>
        );
      })}

      {selectedTargets.length > 0 && !plan && (
        <div className="actionbar">
          <span className="figures">
            {selectedTargets.length} location{selectedTargets.length === 1 ? "" : "s"} selected ·{" "}
            <b>{formatBytes(selectedBytes)}</b> reclaimable
          </span>
          <span className="spacer" />
          <button className="btn" onClick={() => setSelected(new Set())} disabled={busy}>
            Clear selection
          </button>
          <button className="btn primary" onClick={askToClean} disabled={busy}>
            Review and clean
          </button>
        </div>
      )}

      {plan && (
        <ConfirmDialog
          plan={plan}
          targets={selectedTargets}
          useTrash={useTrash}
          busy={busy}
          onCancel={() => setPlan(null)}
          onConfirm={confirmClean}
        />
      )}
    </div>
  );
}