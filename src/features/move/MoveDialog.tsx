import { closeMove, confirmMove, refusalText, selectDrive, setAck, startMove, useMove } from "../../app/moves";
import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import modal from "../../components/Modal.module.css";
import { Segmented } from "../../components/Segmented";
import { formatBytes, formatPercent } from "../../core/format";
import { useT } from "../../i18n";
import type { MoveCheck } from "../../ipc/types";
import styles from "./MoveDialog.module.css";

function Check({ c }: { c: MoveCheck }) {
  const t = useT();
  const size = (n: number) => formatBytes(n, t.locale);
  const ok = c.planId !== null;
  return (
    <>
      <p className={styles.text}>{t("move.text")}</p>
      <dl className={styles.summary}>
        <dt>{t("move.source")}</dt>
        <dd className={styles.mono}>{c.source}</dd>
        {c.target && (
          <>
            <dt>{t("move.target")}</dt>
            <dd className={styles.mono}>{c.target}</dd>
          </>
        )}
        <dt>{t("move.size")}</dt>
        <dd>
          <span className={styles.mono}>{size(c.bytes)}</span> · {t.plural("move.files", c.files)}
        </dd>
        {c.drive && (
          <>
            <dt>{t("move.free")}</dt>
            <dd>
              <span className={styles.mono}>{size(c.free)}</span>
              <span className={styles.sub}>{t("move.needed", { size: size(c.needed) })}</span>
            </dd>
          </>
        )}
      </dl>
      {c.drives.length > 1 && c.drive && (
        <div className={styles.row}>
          <span>{t("move.drive")}</span>
          <Segmented
            label={t("move.drive")}
            value={c.drive}
            onChange={selectDrive}
            options={c.drives.map((d) => ({ value: d.letter, label: `${d.letter} · ${size(d.free)}` }))}
          />
        </div>
      )}
      {c.drives.length === 0 && <p className={styles.warn}>{t("move.noDrive")}</p>}
      {ok && c.review && <p className={styles.warn}>{t("move.review")}</p>}
      {!ok && c.refusals.length > 0 && (
        <div className={styles.warn} role="alert">
          {t("move.refused")}
          <ul className={styles.refusals}>
            {c.refusals.map((r, i) => (
              <li key={`${r.code}-${i}`}>{refusalText(r, t)}</li>
            ))}
          </ul>
        </div>
      )}
      <div className={modal.actions}>
        <Button variant="link" onClick={closeMove}>
          {t("move.cancel")}
        </Button>
        <Button variant="primary" disabled={!ok} onClick={() => void startMove()}>
          {t("move.go")}
        </Button>
      </div>
    </>
  );
}

/** Déplacement d'un dossier vers un autre disque, de la vérification à la décision sur l'original. */
export function MoveDialog() {
  const t = useT();
  const ui = useMove();
  if (ui.step === "closed") return null;
  const size = (n: number) => formatBytes(n, t.locale);
  const p = ui.progress;
  const ratio = p && p.total > 0 ? Math.min(1, p.done / p.total) : 0;
  const busy = ui.step === "running" || ui.step === "ready" || ui.step === "finishing";
  return (
    <Modal title={t("move.title")} onClose={busy ? undefined : closeMove} step={ui.step}>
      {ui.step === "checking" && <p className={styles.hint}>{t("move.checking")}</p>}
      {ui.step === "check" && ui.check && <Check c={ui.check} />}
      {ui.step === "running" && (
        <>
          <div className={styles.step} role="status">
            <span>{p ? t(`move.step.${p.step}`) : t("move.step.recheck")}</span>
            <span className={styles.mono}>{formatPercent(ratio, t.locale)}</span>
          </div>
          <div className={styles.bar} aria-hidden="true">
            <span style={{ width: `${Math.round(ratio * 100)}%` }} />
          </div>
          <p className={styles.hint}>{t("move.running")}</p>
        </>
      )}
      {(ui.step === "ready" || ui.step === "finishing") && ui.run && (
        <>
          <p className={styles.done}>{t("move.ready", { size: size(ui.run.bytes) })}</p>
          <p className={styles.warn}>{t("move.deleteWarning", { target: ui.run.target })}</p>
          <label className={styles.ack}>
            <input type="checkbox" checked={ui.ack} disabled={ui.step === "finishing"} onChange={() => setAck(!ui.ack)} />
            {t("move.ack")}
          </label>
          <div className={modal.actions}>
            <Button variant="link" disabled={ui.step === "finishing"} onClick={() => void confirmMove(false)}>
              {t("move.rollback")}
            </Button>
            <Button variant="primary" disabled={!ui.ack || ui.step === "finishing"} onClick={() => void confirmMove(true)}>
              {t("move.delete", { size: size(ui.run.bytes) })}
            </Button>
          </div>
        </>
      )}
      {ui.step === "result" && ui.result && (
        <>
          <p className={ui.result.kind === "done" ? styles.done : styles.warn} role="status">
            {ui.result.text}
          </p>
          <div className={modal.actions}>
            <Button variant="primary" onClick={closeMove}>
              {t("move.close")}
            </Button>
          </div>
        </>
      )}
    </Modal>
  );
}
