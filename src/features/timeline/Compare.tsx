import { openPath } from "../../app/navigation";
import { useStore } from "../../app/store";
import { runCompare, setCompare, useTimeline } from "../../app/timeline";
import { Button } from "../../components/Button";
import { formatBytes, formatDateTime, formatDelta } from "../../core/format";
import { type Translator, useT } from "../../i18n";
import type { FolderChange, ScanTarget, TimelineCompare, TimelinePoint, TimelineSeries } from "../../ipc/types";
import { seriesLabel } from "./labels";
import styles from "./Timeline.module.css";

interface Props {
  /** Snapshots de la série affichée, sur la période. */
  points: TimelinePoint[];
  /** Autres séries du même disque : leur dernier snapshot, pour montrer qu'on ne les compare pas. */
  others: TimelineSeries[];
}

function List({ title, changes, result, kind, t }: { title: string; changes: FolderChange[]; result: TimelineCompare; kind: "grown" | "shrunk" | "added" | "removed"; t: Translator }) {
  const showToast = useStore((s) => s.showToast);
  const open = (c: FolderChange, base: string, target: ScanTarget) => {
    const full = `${base.replace(/\\$/, "")}\\${c.path}`;
    void openPath(target, full).then((ok) => {
      if (!ok) showToast(t("timeline.diff.notFound", { path: c.path }));
    });
  };
  return (
    <section className={styles.diffList} aria-label={title}>
      <h4 className={styles.diffTitle}>{title}</h4>
      {changes.length === 0 ? (
        <p className={styles.diffNone}>{t("timeline.diff.none")}</p>
      ) : (
        <ul className={styles.rows}>
          {changes.map((c) => {
            const delta = <span className={c.delta > 0 ? styles.up : styles.down}>{formatDelta(c.delta, t.locale)}</span>;
            const gone = kind === "removed";
            const { base, target } = result;
            return (
              <li key={c.path}>
                {gone || base === null || target === null ? (
                  <span className={styles.row} title={gone ? t("timeline.diff.gone", { path: c.path }) : undefined}>
                    <code className={styles.rowPath}>{c.path}</code>
                    {delta}
                  </span>
                ) : (
                  <button type="button" className={`${styles.row} ${styles.rowLink}`} onClick={() => open(c, base, target)} title={t("timeline.diff.open", { path: c.path })}>
                    <code className={styles.rowPath}>{c.path}</code>
                    {delta}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** Comparaison de deux dates : top des dossiers qui ont grossi ou rétréci, nouveaux, disparus. */
export function Compare({ points, others }: Props) {
  const t = useT();
  const { from, to, compare } = useTimeline();
  const a = from ?? points[0]?.id ?? null;
  const b = to ?? points[points.length - 1]?.id ?? null;
  const method = (m: string) => t(m === "mft" ? "timeline.method.mft" : "timeline.method.walk");
  const rootName = (r: string) => (r === "\\" ? t("timeline.root.volume") : r);
  const result = compare.status === "done" ? compare.value : null;

  return (
    <section className={styles.compare} aria-label={t("timeline.compare")}>
      <div className={styles.compareBar}>
        <h3 className={styles.sectionTitle}>{t("timeline.compare")}</h3>
        <label className={styles.field}>
          <span>{t("timeline.compare.from")}</span>
          <select className={styles.select} value={a ?? ""} onChange={(e) => setCompare(Number(e.target.value), b)}>
            {points.map((p) => (
              <option key={p.id} value={p.id}>
                {formatDateTime(p.at, t.locale)} · {formatBytes(p.used, t.locale)}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.field}>
          <span>{t("timeline.compare.to")}</span>
          <select className={styles.select} value={b ?? ""} onChange={(e) => setCompare(a, Number(e.target.value))}>
            {points.map((p) => (
              <option key={p.id} value={p.id}>
                {formatDateTime(p.at, t.locale)} · {formatBytes(p.used, t.locale)}
              </option>
            ))}
            {others.length > 0 && (
              <optgroup label={t("timeline.compare.others")}>
                {others.map((s) => (
                  <option key={s.lastId} value={s.lastId}>
                    {seriesLabel(s, t)} · {formatDateTime(s.last, t.locale)}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </label>
        <Button variant="primary" disabled={a === null || b === null || a === b || compare.status === "loading"} onClick={() => a !== null && b !== null && void runCompare(a, b)}>
          {t("timeline.compare.run")}
        </Button>
      </div>
      {a !== null && a === b && <p className={styles.hint}>{t("timeline.compare.same")}</p>}
      {compare.status === "failed" && <p className={styles.warning}>{compare.message}</p>}
      {result && !result.comparable && (
        <p className={styles.warning} role="note">
          {result.reason === "method"
            ? t("timeline.compare.method", { a: method(result.from.method), b: method(result.to.method) })
            : result.reason === "root"
              ? t("timeline.compare.root", { a: rootName(result.from.root), b: rootName(result.to.root) })
              : t("timeline.compare.volume")}
        </p>
      )}
      {result?.diff && (
        <>
          <p className={styles.total}>
            {t("timeline.compare.total", {
              delta: formatDelta(result.diff.total, t.locale),
              from: formatDateTime(result.from.at, t.locale),
              to: formatDateTime(result.to.at, t.locale),
            })}
          </p>
          <div className={styles.diff}>
            <List title={t("timeline.diff.grown")} changes={result.diff.grown} result={result} kind="grown" t={t} />
            <List title={t("timeline.diff.shrunk")} changes={result.diff.shrunk} result={result} kind="shrunk" t={t} />
            <List title={t("timeline.diff.added")} changes={result.diff.added} result={result} kind="added" t={t} />
            <List title={t("timeline.diff.removed")} changes={result.diff.removed} result={result} kind="removed" t={t} />
          </div>
          <p className={styles.hint}>
            {t("timeline.diff.threshold", { size: formatBytes(result.minBytes, t.locale) })}
            {result.base === null && ` ${t("timeline.diff.absent")}`}
          </p>
        </>
      )}
    </section>
  );
}
