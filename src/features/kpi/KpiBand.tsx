import { useKpis } from "../../app/data";
import { useStore } from "../../app/store";
import { TrendIcon } from "../../components/icons";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatDelta } from "../../core/format";
import { useT } from "../../i18n";
import styles from "./KpiBand.module.css";

export function KpiBand() {
  const t = useT();
  const volume = useStore((s) => s.volume);
  const go = useStore((s) => s.go);
  const kpis = useKpis(volume);
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
          {kpis && (
            <>
              <span className={styles.gain}>{formatDelta(kpis.safeBytes, t.locale)}</span>
              {" · "}
              {t("kpi.freeToday", { size: size(kpis.freeNow) })}
            </>
          )}
        </span>
      </div>
      <div className={styles.cell}>
        <span className={styles.label}>
          <StatusDot status="safe" />
          {t("kpi.safe")}
        </span>
        <span className={styles.value}>{kpis ? size(kpis.safeBytes) : "—"}</span>
        <span className={styles.sub}>{kpis && t.plural("kpi.safeSub", kpis.safeCount)}</span>
      </div>
      <div className={styles.cell}>
        <span className={styles.label}>
          <StatusDot status="review" />
          {t("kpi.review")}
        </span>
        <span className={styles.value}>{kpis ? size(kpis.reviewBytes) : "—"}</span>
        <span className={styles.sub}>{kpis && t.plural("kpi.reviewSub", kpis.reviewCount)}</span>
      </div>
      <div className={styles.cell}>
        <span className={styles.label}>
          <StatusDot status="outline" />
          {t("kpi.unanalyzed")}
        </span>
        <span className={`${styles.value} ${styles.muted}`}>{kpis ? size(kpis.unanalyzed) : "—"}</span>
        <span className={styles.sub}>{t("kpi.unanalyzedSub")}</span>
      </div>
    </section>
  );
}
