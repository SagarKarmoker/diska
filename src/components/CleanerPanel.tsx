import { useMemo, useState } from "react";

import { Empty, ErrorBox, RiskBadge, Section } from "./primitives";
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

/**
 * Confirmation dialog. Deleting is irreversible, so the user sees the exact
 * paths and byte totals, and permanent deletion is spelled out separately from
 * moving to the trash.
 */
function ConfirmDialog({
  plan,
  targets,
  useTrash,
  onCancel,
  onConfirm,
  busy,
}: {
  plan: CleanPlan;
  targets: JunkTarget[];
  useTrash: boolean;
  onCancel: () => void;
  onConfirm: () => void;
  busy: boolean;
}) {
  const elevated = targets.filter((t) => t.needsElevation);
  const cautious = targets.filter((t) => t.risk === "caution");

  return (
    <div className="backdrop" role="dialog" aria-modal="true" aria-label="Confirm clean">
      <div className="dialog">
        <h2>{useTrash ? "Move to trash?" : "Delete permanently?"}</h2>
        <p className="note">
          {useTrash
            ? "The files move to your system trash and can be restored from there."
            : "This removes the files immediately. There is no trash and no undo."}
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
        </div>

        {!useTrash && (
          <div className="callout danger">
            Permanent deletion cannot be undone. If you are unsure, choose the trash instead.
          </div>
        )}

        {cautious.length > 0 && (
          <div className="callout">
            {cautious.length} of these hold real data rather than regenerable cache
            {cautious.length === 1 ? "" : "s"}:{" "}
            {cautious.map((t) => t.label).join(", ")}. Each is marked for review above.
          </div>
        )}

        {elevated.length > 0 && (
          <div className="callout">
            {elevated.length} location(s) need administrator rights and will be skipped.
          </div>
        )}

        <ul className="paths">
          {targets.map((t) => (
            <li key={t.id}>
              {formatBytes(t.size).padStart(9)}  {t.path}
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
      <div className="results">
        <span>Freed {formatBytes(outcome.freedBytes)}</span>
        <span>{formatCount(outcome.removedCount)} items removed</span>
        {outcome.skipped.length > 0 && (
          <span>{formatCount(outcome.skipped.length)} skipped</span>
        )}
        {outcome.failures.length > 0 && (
          <span>{formatCount(outcome.failures.length)} failed</span>
        )}
      </div>

      {(outcome.skipped.length > 0 || outcome.failures.length > 0) && (
        <details>
          <summary>Details</summary>
          <ul className="paths">
            {outcome.failures.map((f) => (
              <li key={`f-${f.path}`}>
                failed: {f.path} — {f.reason}
              </li>
            ))}
            {outcome.skipped.map((s) => (
              <li key={`s-${s.path}`}>
                skipped: {s.path} — {s.reason}
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

  const detect = async () => {
    setBusy(true);
    setError(null);
    setOutcome(null);
    setProgress(null);

    try {
      const found = await detectJunk(includeStale, setProgress);
      setTargets(found);
      // Only fully regenerable caches start selected. Rebuildable items are
      // slower to restore and "review" items may hold real data, so both stay
      // off until the user opts in.
      setSelected(new Set(found.filter((t) => t.risk === "safe" && !t.needsElevation).map((t) => t.id)));
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
    return [...map.entries()].sort((a, b) => {
      const sa = a[1].reduce((n, t) => n + t.size, 0);
      const sb = b[1].reduce((n, t) => n + t.size, 0);
      return sb - sa;
    });
  }, [targets]);

  const selectedTargets = useMemo(
    () => targets?.filter((t) => selected.has(t.id)) ?? [],
    [targets, selected],
  );

  const selectedBytes = selectedTargets.reduce((n, t) => n + t.size, 0);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleCategory = (ids: string[], on: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  };

  const askToClean = async () => {
    setBusy(true);
    setError(null);
    try {
      // Dry run first so the dialog quotes numbers from the same code path that
      // performs the clean, not from a stale list in the UI.
      const dry = await dryRunClean([...selected], useTrash, includeStale);
      setOutcome(dry);
      setPlan(
        await previewClean([...selected], includeStale),
      );
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
      const result = await runClean([...selected], useTrash, includeStale);
      setOutcome(result);
      setPlan(null);
      // Sizes are now stale; re-detect so the list reflects reality.
      const found = await detectJunk(includeStale, () => undefined);
      setTargets(found);
      setSelected(new Set(found.filter((t) => t.risk === "safe" && !t.needsElevation).map((t) => t.id)));
    } catch (e) {
      setError(errorMessage(e));
      setPlan(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      {error && <ErrorBox message={error} />}

      <Section title="Clean">
        <p className="note">
          Everything is detected by scanning well-known cache locations. Only fully regenerable
          items are pre-selected. Nothing is removed without a confirmation that lists each path,
          and files go to your trash unless you explicitly choose permanent deletion.
        </p>

        <div className="toolbar">
          <label className="check">
            <input
              type="checkbox"
              checked={includeStale}
              onChange={(e) => setIncludeStale(e.target.checked)}
            />
            Aggressive sweep: old installers, stale logs, build output
          </label>
          <button className="btn primary" onClick={detect} disabled={busy}>
            {busy ? "Detecting…" : targets ? "Rescan" : "Find junk"}
          </button>
          <label className="check">
            <input
              type="checkbox"
              checked={useTrash}
              onChange={(e) => setUseTrash(e.target.checked)}
            />
            Move to trash instead of deleting
          </label>
        </div>

        {busy && progress && (
          <div className="results">
            <span>{progress.phase}</span>
            <span>{progress.label}</span>
            <span>
              {formatCount(progress.targetsFound)} found, {formatBytes(progress.bytesSoFar)}
            </span>
          </div>
        )}

        {outcome && <Outcome outcome={outcome} />}
      </Section>

      {targets && targets.length === 0 && (
        <Empty>Nothing cleanable found. Your caches are already tidy.</Empty>
      )}

      {grouped.map(([category, items]) => {
        const categoryBytes = items.reduce((n, t) => n + t.size, 0);
        const ids = items.map((t) => t.id);
        const allOn = ids.every((id) => selected.has(id));

        return (
          <div className="category" key={category}>
            <header>
              <label className="check">
                <input
                  type="checkbox"
                  checked={allOn}
                  onChange={(e) => toggleCategory(ids, e.target.checked)}
                />
                <span className="name">{category}</span>
              </label>
              <span className="figures">
                {formatBytes(categoryBytes)} · {items.length} item(s)
              </span>
            </header>

            {items.map((t) => {
              const on = selected.has(t.id);
              return (
                <div className={`target ${on ? "" : "is-off"}`} key={t.id}>
                  <input
                    type="checkbox"
                    checked={on}
                    disabled={t.needsElevation}
                    onChange={() => toggle(t.id)}
                    aria-label={`Clean ${t.label}`}
                  />
                  <div>
                    <div className="label">
                      <span>{t.label}</span>
                      <RiskBadge risk={t.risk} />
                      {t.needsElevation && <span className="tag">needs admin</span>}
                    </div>
                    <div className="reason">{t.reason}</div>
                    <div className="where">
                      {t.path} · {formatCount(t.entryCount)} entries
                    </div>
                  </div>
                  <div className="size">{formatBytes(t.size)}</div>
                </div>
              );
            })}
          </div>
        );
      })}

      {selectedTargets.length > 0 && !plan && (
        <div className="toolbar">
          <span>
            {selectedTargets.length} location(s), {formatBytes(selectedBytes)} reclaimable
          </span>
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