import { useMemo } from "react";
import { cancelScan } from "../../app/scanning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import pop from "../../components/Popover.module.css";
import { formatBytes, formatCount } from "../../core/format";
import { type MessageKey, useT } from "../../i18n";
import type { TopProgress, TreeNode } from "../../ipc/types";
import { Treemap } from "./Treemap";
import styles from "./ScanPanel.module.css";

function seconds(ms: number, locale: "fr" | "en"): string {
  return `${new Intl.NumberFormat(locale === "fr" ? "fr-FR" : "en-US", { maximumFractionDigits: 1, minimumFractionDigits: 1 }).format(ms / 1000)} s`;
}

/** Carte en direct pendant la première analyse : les dossiers de premier niveau déjà comptés. */
function liveRoot(tops: TopProgress[]): TreeNode {
  const node = (t: TopProgress, i: number): TreeNode => ({
    id: -1000 - i,
    name: t.name,
    path: t.name,
    kind: "dir",
    size: t.allocated,
    logical: t.allocated,
    othersCount: null,
    files: t.files,
    lastOpened: Date.now(),
    // Pendant l'analyse, aucune règle n'est encore appliquée : couleur neutre.
    category: "system",
    status: "keep",
    reco: null,
    gain: 0,
    gainStatus: null,
    gainUpTo: false,
    hasChildren: false,
    children: [],
  });
  const children = tops.filter((t) => t.allocated > 0).map(node);
  return { ...node({ name: "", allocated: children.reduce((s, c) => s + c.size, 0), files: 0 }, -1), id: -1, children };
}

/** Progression de l'analyse (≈ 10 mises à jour par seconde), annulable. */
export function ScanProgressPanel({ overlay }: { overlay: boolean }) {
  const t = useT();
  const scan = useStore((s) => s.scan);
  const p = scan.progress;
  const live = useMemo(() => (p && !overlay && p.tops.length > 0 ? liveRoot(p.tops) : null), [p, overlay]);

  if (scan.status !== "running") {
    return (
      <div className={styles.idle}>
        <p>{scan.status === "failed" && scan.message ? t("scan.failed", { message: scan.message }) : t("scan.empty")}</p>
      </div>
    );
  }

  const phase = p ? (`scan.phase.${p.phase}` as MessageKey) : null;
  return (
    <div className={overlay ? styles.overlay : styles.full}>
      {live && (
        <div className={styles.live} aria-hidden="true">
          <Treemap root={live} highlight={null} onCopyPath={() => {}} onUnavailable={() => {}} />
        </div>
      )}
      <div className={`${pop.pop} ${styles.card}`} role="status" aria-live="polite">
        <div className={styles.head}>
          <span className={styles.title}>{t("scan.running")}</span>
          {phase && <span className={styles.phase}>{t(phase)}</span>}
        </div>
        <div className={styles.counters}>
          <span>{t.plural("scan.files", p?.files ?? 0, { n: formatCount(p?.files ?? 0, t.locale) })}</span>
          {p && p.allocated > 0 && <span>{formatBytes(p.allocated, t.locale)}</span>}
          {p && <span>{seconds(p.elapsedMs, t.locale)}</span>}
        </div>
        <div className={styles.track} aria-hidden="true">
          <div className={styles.bar} />
        </div>
        <Button variant="link" onClick={cancelScan}>
          {t("scan.cancel")}
        </Button>
      </div>
    </div>
  );
}

/** Ligne de bilan sous la légende : fichiers, cloud, liens, inaccessibles, méthode et durée. */
export function ScanStats() {
  const t = useT();
  const summary = useStore((s) => s.scan.summary);
  if (!summary) return null;
  const n = (v: number) => formatCount(v, t.locale);
  const errors = summary.accessDenied + summary.locked + summary.otherErrors;
  const parts = [
    t.plural("scan.files", summary.files, { n: n(summary.files) }),
    summary.cloudFiles > 0 ? t("scan.cloud", { n: n(summary.cloudFiles), size: formatBytes(summary.cloudBytes, t.locale) }) : null,
    summary.hardlinkDups > 0 ? t("scan.hardlinks", { n: n(summary.hardlinkDups) }) : null,
    summary.links > 0 ? t.plural("scan.links", summary.links, { n: n(summary.links) }) : null,
    errors > 0 ? t.plural("scan.errors", errors, { n: n(errors) }) : null,
    t(summary.method === "mft" ? "scan.method.mft" : "scan.method.walk", { s: seconds(summary.elapsedMs, t.locale) }),
  ].filter(Boolean);
  return (
    <>
      <p className={styles.stats}>{parts.join(" · ")}</p>
      {summary.notice && <p className={styles.notice}>{t(`scan.notice.${summary.notice}`)}</p>}
    </>
  );
}
