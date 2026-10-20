import { useState } from "react";
import { useCopyFlash } from "../../app/clipboard";
import { useRecommendation } from "../../app/data";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { ChevronLeftIcon, CloseSmallIcon, InfoIcon } from "../../components/icons";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes } from "../../core/format";
import { currentT, useT } from "../../i18n";
import styles from "./DetailPanel.module.css";

/** Panneau de détail d'une recommandation : remplace la liste dans la colonne de droite. */
export function DetailPanel({ id }: { id: string }) {
  const t = useT();
  const close = () => useStore.getState().setOpenReco(null);
  const showToast = useStore((s) => s.showToast);
  const reco = useRecommendation(id);
  // Simulation -WhatIf activée par défaut, à chaque ouverture.
  const [whatIf, setWhatIf] = useState(true);
  const [copied, copy] = useCopyFlash();

  if (!reco) return <div className={styles.panel} />;

  const hasWhatIf = reco.script.some((l) => l.whatIfAt !== null);
  const lines = reco.script.map((l) =>
    l.whatIfAt !== null && whatIf ? `${l.text.slice(0, l.whatIfAt)} -WhatIf${l.text.slice(l.whatIfAt)}` : l.text,
  );
  const max = Math.max(...reco.files.map((f) => f.size), 1);
  const size = formatBytes(reco.size, t.locale);

  return (
    <div className={styles.panel}>
      <div className={styles.top}>
        <button type="button" className={styles.back} onClick={close}>
          <ChevronLeftIcon />
          {t("detail.back")}
        </button>
        <button type="button" className={styles.close} aria-label={t("detail.close")} onClick={close}>
          <CloseSmallIcon />
        </button>
      </div>

      <div className={styles.intro}>
        <div className={styles.titleRow}>
          <div className={styles.titleBlock}>
            <span className={styles.status}>
              <StatusDot status={reco.status} />
              {t(`status.${reco.status}`)}
            </span>
            <h2 className={styles.name}>{reco.name}</h2>
            <span className={styles.path}>{reco.path}</span>
          </div>
          <span className={styles.size}>{size}</span>
        </div>
        <p className={styles.explanation}>{reco.explanation}</p>
        <span className={styles.meta}>{reco.meta}</span>
      </div>

      <section className={styles.section}>
        <div className={styles.sectionHead}>
          <h3 className={styles.sectionTitle}>{t("detail.files")}</h3>
          <span className={styles.count}>{t.plural("detail.files", reco.files.length)}</span>
        </div>
        <ul className={styles.files}>
          {reco.files.map((f) => (
            <li key={f.path} className={styles.file}>
              <div className={styles.fileRow}>
                <span className={styles.filePath} title={f.path}>
                  {f.path}
                </span>
                <span className={styles.fileSize}>{formatBytes(f.size, t.locale)}</span>
              </div>
              <div aria-hidden="true" className={styles.track}>
                <div className={styles.bar} data-status={reco.status} style={{ width: `${Math.max(3, Math.round((f.size / max) * 100))}%` }} />
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className={`${styles.section} ${styles.scriptSection}`}>
        <div className={styles.sectionHead}>
          <h3 className={styles.sectionTitle}>
            {t("detail.script")} <span className={styles.lang}>· PowerShell</span>
          </h3>
          {hasWhatIf && (
            <label className={styles.whatIf}>
              <input type="checkbox" checked={whatIf} onChange={() => setWhatIf(!whatIf)} />
              {t("detail.whatIf")} <span className={styles.lang}>-WhatIf</span>
            </label>
          )}
        </div>
        <pre className={styles.script} tabIndex={0}>
          {lines.map((text, i) => (
            <span key={i} className={styles.line}>
              <span aria-hidden="true" className={styles.lineNo}>
                {i + 1}
              </span>
              <span className={text.trim().startsWith("#") ? styles.comment : undefined}>{text}</span>
            </span>
          ))}
        </pre>
        <span className={styles.note}>
          <InfoIcon />
          {reco.note}
        </span>
      </section>

      <div className={styles.footer}>
        <Button variant="link" onClick={() => showToast(currentT()("preview.later", { n: 3 }))}>
          {t("detail.reveal")}
        </Button>
        <div className={styles.buttons}>
          <Button
            variant="primary"
            onClick={() =>
              void copy(lines.join("\n")).then(
                (ok) => ok && showToast(currentT()(hasWhatIf && whatIf ? "preview.scriptCopied" : "preview.scriptCopiedReal")),
              )
            }
          >
            {copied ? t("reco.copied") : t("reco.copyScript")}
          </Button>
          <Button variant="clean" onClick={() => showToast(currentT()("preview.later", { n: 5 }))}>
            {reco.status === "safe" ? t("detail.cleanSafe", { size }) : t("detail.cleanReview")}
          </Button>
        </div>
      </div>
    </div>
  );
}
