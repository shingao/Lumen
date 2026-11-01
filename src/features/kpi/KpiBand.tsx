import { useEffect } from "react";
import { useKpis } from "../../app/data";
import { loadCachedDepth, startDepth, systemDrive, useDepth } from "../../app/depth";
import { useStore } from "../../app/store";
import { TrendIcon } from "../../components/icons";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatDelta, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import { StatusHelp } from "../status/StatusHelp";
import styles from "./KpiBand.module.css";

export function KpiBand() {
  const t = useT();
  const go = useStore((s) => s.go);
  const kpis = useKpis();
  // Sans moteur de règles (jalon 4), pas encore de « sans risque » ni de « à vérifier ».
  const rules = kpis?.rulesReady ?? false;
  const size = (n: number) => formatBytes(n, t.locale);

  return (
    <section aria-label={t("kpi.label")} className={styles.band}>
      <div className={styles.cell}>
        <span className={styles.labelRow}>
          <span className={styles.label}>{t("kpi.freeAfter")}</span>
          {kpis && kpis.trend7d !== 0 && (
            <button type="button" className={styles.trend} title={t("kpi.trendLabel")} aria-label={t("kpi.trendLabel")} onClick={() => go("timeline")}>
              <TrendIcon />
              {t("kpi.trend", { delta: formatDelta(kpis.trend7d, t.locale) })}
            </button>
          )}
        </span>
        <span className={styles.value}>{kpis ? size(kpis.freeAfter) : "—"}</span>
        <span className={styles.sub}>
          {kpis &&
            (rules ? (
              <>
                <span className={styles.gain}>{formatDelta(kpis.safeBytes, t.locale)}</span>
                {" · "}
                {t("kpi.freeToday", { size: size(kpis.freeNow) })}
              </>
            ) : (
              t("kpi.freeToday", { size: size(kpis.freeNow) })
            ))}
        </span>
      </div>
      <div className={styles.cell}>
        <span className={styles.label}>
          <StatusDot status="safe" />
          {t("kpi.safe")}
          {rules && <StatusHelp status="safe" from="kpi" />}
        </span>
        <span className={styles.value}>{kpis && rules ? size(kpis.safeBytes) : "—"}</span>
        <span className={styles.sub}>{kpis && (rules ? t.plural("kpi.safeSub", kpis.safeCount) : t("kpi.rulesPending"))}</span>
      </div>
      <div className={styles.cell}>
        <span className={styles.label}>
          <StatusDot status="review" />
          {t("kpi.review")}
          {rules && <StatusHelp status="review" from="kpi" />}
        </span>
        <span className={styles.value}>{kpis && rules ? size(kpis.reviewBytes) : "—"}</span>
        <span className={styles.sub}>{kpis && (rules ? t.plural("kpi.reviewSub", kpis.reviewCount) : t("kpi.rulesPending"))}</span>
      </div>
      <Unanalyzed value={kpis ? size(kpis.unanalyzed) : "—"} />
    </section>
  );
}

/** « Non analysé » : bouton « Analyser en profondeur », puis « Analysé en profondeur il y a X ». */
function Unanalyzed({ value }: { value: string }) {
  const t = useT();
  const go = useStore((s) => s.go);
  const status = useDepth((s) => s.status);
  const finishedAt = useDepth((s) => (s.report?.volume.toUpperCase() === systemDrive() ? s.report.finishedAt : null));
  useEffect(() => {
    void loadCachedDepth();
  }, []);
  const open = () => go("depth");
  return (
    <div className={styles.cell}>
      <span className={styles.label}>
        <StatusDot status="outline" />
        {finishedAt !== null ? t("kpi.depthDone", { when: formatRelative(finishedAt, t.locale) }) : t("kpi.unanalyzed")}
      </span>
      <span className={`${styles.value} ${styles.muted}`}>{value}</span>
      <span className={styles.subRow}>
        <span className={styles.sub}>{status === "running" ? t("kpi.depthRunning") : t("kpi.unanalyzedSub")}</span>
        {finishedAt !== null || status === "running" ? (
          <button type="button" className={styles.action} onClick={open}>
            {t("kpi.depthOpen")}
          </button>
        ) : (
          <button
            type="button"
            className={styles.action}
            onClick={() => {
              open();
              void startDepth("auto");
            }}
          >
            {t("kpi.depthStart")}
          </button>
        )}
      </span>
    </div>
  );
}
