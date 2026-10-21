import { useEffect } from "react";
import { checkDormant, loadDormant, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatCount } from "../../core/format";
import { useT } from "../../i18n";
import type { DormantResult } from "../../ipc/types";
import { Check, FileLine, SelectionBar } from "./FileRows";
import styles from "./Advanced.module.css";

const MONTHS = [3, 6, 12, 24] as const;

/** Valeur brute du registre, telle que l'affiche `fsutil` (0x80000002…). */
function rawValue(raw: number | null, unknown: string): string {
  return raw === null ? unknown : `0x${raw.toString(16).toUpperCase()}`;
}

function Folders({ result }: { result: DormantResult }) {
  const t = useT();
  const checked = useAdvanced((s) => s.checkedDormant);
  return (
    <div className={styles.groups}>
      {result.folders.map((f) => {
        const ids = f.files.map((x) => x.id);
        const n = ids.filter((id) => checked.has(id)).length;
        return (
          <section key={f.path} className={styles.group} aria-label={f.path}>
            <div className={styles.groupHead}>
              <label className={styles.groupLabel}>
                <Check checked={n === ids.length && n > 0} partial={n > 0} label={t("dormant.checkFolder", { path: f.path })} onChange={(on) => checkDormant(ids, on)} />
                <code className={styles.groupPath}>{f.path}</code>
              </label>
              <span className={styles.groupMeta}>{t.plural("dormant.files", f.count, { n: formatCount(f.count, t.locale) })}</span>
              <span className={styles.groupSize}>{formatBytes(f.size, t.locale)}</span>
            </div>
            <ul className={styles.rows}>
              {f.files.map((file) => (
                <FileLine
                  key={file.id}
                  file={file}
                  date={result.basis === "lastAccess" ? "accessed" : "modified"}
                  checked={checked.has(file.id)}
                  onChange={(on) => checkDormant([file.id], on)}
                />
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

/** Fichiers pas ouverts (ou pas modifiés) depuis N mois : toujours « À vérifier ». */
export function DormantTab() {
  const t = useT();
  const defaultMonths = useStore((s) => s.settings?.dormantMonths ?? 6);
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  const { dormant, dormantMonths, checkedDormant, scanId: loadedFor } = useAdvanced();
  const months = dormantMonths ?? defaultMonths;

  useEffect(() => {
    if (scanId !== null && (loadedFor !== scanId || dormant.status === "idle")) void loadDormant(months);
  }, [scanId, loadedFor, dormant.status, months]);

  const result = dormant.status === "done" ? dormant.value : null;
  const files = result?.folders.flatMap((f) => f.files) ?? [];
  const selected = files.filter((f) => checkedDormant.has(f.id));
  const size = selected.reduce((a, f) => a + f.size, 0);
  const options = MONTHS.map((m) => ({ value: m, label: t("dormant.months.n", { n: m }) }));

  return (
    <>
      <div className={styles.toolbar}>
        <span className={styles.toolLabel}>{t("dormant.months")}</span>
        <Segmented<number> options={options} value={months} onChange={(m) => void loadDormant(m)} label={t("dormant.months")} />
      </div>
      {result && (
        <p className={result.basis === "lastAccess" ? styles.notice : styles.warning} role={result.basis === "lastWrite" ? "note" : undefined}>
          {t(result.basis === "lastAccess" ? "dormant.basis.lastAccess" : "dormant.basis.lastWrite", { raw: rawValue(result.lastAccessRaw, t("dormant.basis.unknown")) })}
        </p>
      )}
      {dormant.status === "loading" && <p className={styles.hint}>{t("adv.loading")}</p>}
      {dormant.status === "failed" && <p className={styles.warning}>{dormant.message}</p>}
      {result && (
        <>
          <div className={styles.summary}>
            <span className={styles.status}>
              <StatusDot status="review" />
              {t("status.review")}
            </span>
            <span className={styles.summaryText}>
              {t.plural("dormant.summary", result.count, { n: formatCount(result.count, t.locale), months: result.months, size: formatBytes(result.total, t.locale) })}
            </span>
          </div>
          <p className={styles.hint}>
            {t("adv.note")} {t("dormant.excluded")}
          </p>
          {result.folders.length === 0 ? <p className={styles.empty}>{t("adv.empty")}</p> : <Folders result={result} />}
        </>
      )}
      <SelectionBar
        summary={t.plural("adv.selected", selected.length, { n: formatCount(selected.length, t.locale), size: formatBytes(size, t.locale) })}
        onClear={() => checkDormant([...checkedDormant], false)}
        action={t("adv.clean")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "dormant", ids: selected.map((f) => f.id) })}
      />
    </>
  );
}
