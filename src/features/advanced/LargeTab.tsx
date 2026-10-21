import { useEffect } from "react";
import { checkLargest, loadLargest, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatCount } from "../../core/format";
import { useT } from "../../i18n";
import type { FileKind } from "../../ipc/types";
import { FileLine, SelectionBar } from "./FileRows";
import styles from "./Advanced.module.css";

type Filter = FileKind | "all";

/** Les 100 plus gros fichiers de l'analyse, filtrables par type : toujours « À vérifier ». */
export function LargeTab() {
  const t = useT();
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  const { largest, largestKind, checkedLargest, scanId: loadedFor } = useAdvanced();

  useEffect(() => {
    if (scanId !== null && (loadedFor !== scanId || largest.status === "idle")) void loadLargest(largestKind);
  }, [scanId, loadedFor, largest.status, largestKind]);

  const result = largest.status === "done" ? largest.value : null;
  const files = result?.files ?? [];
  const selected = files.filter((f) => checkedLargest.has(f.id));
  const size = selected.reduce((a, f) => a + f.size, 0);
  const total = files.reduce((a, f) => a + f.size, 0);
  const options: { value: Filter; label: string }[] = [
    { value: "all", label: t("large.all") },
    ...(result?.kinds ?? []).map(([k, n]) => ({ value: k as Filter, label: `${t(`large.kind.${k}`)} · ${formatCount(n, t.locale)}` })),
  ];

  return (
    <>
      <div className={styles.toolbar}>
        <span className={styles.toolLabel}>{t("large.filter")}</span>
        <Segmented<Filter> options={options} value={largestKind ?? "all"} onChange={(k) => void loadLargest(k === "all" ? null : k)} label={t("large.filter")} />
      </div>
      {largest.status === "loading" && <p className={styles.hint}>{t("adv.loading")}</p>}
      {largest.status === "failed" && <p className={styles.warning}>{largest.message}</p>}
      {result && (
        <>
          <div className={styles.summary}>
            <span className={styles.status}>
              <StatusDot status="review" />
              {t("status.review")}
            </span>
            <span className={styles.summaryText}>{t.plural("large.summary", files.length, { n: formatCount(files.length, t.locale), size: formatBytes(total, t.locale) })}</span>
          </div>
          <p className={styles.hint}>
            {t("adv.note")} {t("large.note")}
          </p>
          {files.length === 0 ? (
            <p className={styles.empty}>{t("adv.empty")}</p>
          ) : (
            <ul className={`${styles.rows} ${styles.list}`}>
              {files.map((f) => (
                <FileLine key={f.id} file={f} checked={checkedLargest.has(f.id)} onChange={(on) => checkLargest([f.id], on)} />
              ))}
            </ul>
          )}
        </>
      )}
      <SelectionBar
        summary={t.plural("adv.selected", selected.length, { n: formatCount(selected.length, t.locale), size: formatBytes(size, t.locale) })}
        onClear={() => checkLargest([...checkedLargest], false)}
        action={t("adv.clean")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "largest", ids: selected.map((f) => f.id) })}
      />
    </>
  );
}
