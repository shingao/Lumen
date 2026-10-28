import { closeExport, runExport, setExport, useExport, type ReportFormat } from "../../app/report";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import modal from "../../components/Modal.module.css";
import { Segmented } from "../../components/Segmented";
import { useT } from "../../i18n";
import { backend } from "../../ipc/client";
import styles from "./ExportDialog.module.css";

/** Export du rapport : HTML autonome ou CSV, chemins anonymisés par défaut. */
export function ExportDialog() {
  const t = useT();
  const ui = useExport();
  const hasScan = useStore((s) => s.scan.summary !== null);
  if (!ui.open) return null;
  return (
    <Modal title={t("export.title")} onClose={ui.busy ? undefined : closeExport} step={ui.saved ? "saved" : "options"}>
      <p className={styles.text}>{t("export.text")}</p>
      <div className={styles.row}>
        <span>{t("export.format")}</span>
        <Segmented<ReportFormat>
          label={t("export.format")}
          value={ui.format}
          onChange={(format) => setExport({ format })}
          options={[
            { value: "html", label: t("export.html") },
            { value: "csv", label: t("export.csv") },
          ]}
        />
      </div>
      <label className={styles.check}>
        <input type="checkbox" checked={ui.anonymize} onChange={() => setExport({ anonymize: !ui.anonymize })} />
        <span>
          {t("export.anonymize")}
          <small>{t("export.anonymizeHint")}</small>
        </span>
      </label>
      {ui.saved && (
        <p className={styles.saved} role="status">
          {t("export.saved")} <b>{ui.saved.name}</b>
        </p>
      )}
      {ui.message && <p className={styles.error}>{ui.message}</p>}
      <div className={modal.actions}>
        {ui.saved ? (
          <>
            <Button variant="link" onClick={() => void backend.revealReport()}>
              {t("export.reveal")}
            </Button>
            <Button variant="primary" onClick={closeExport}>
              {t("export.close")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="link" onClick={closeExport} disabled={ui.busy}>
              {t("export.cancel")}
            </Button>
            <Button variant="primary" onClick={() => void runExport()} disabled={ui.busy || !hasScan}>
              {ui.busy ? t("export.busy") : t("export.go")}
            </Button>
          </>
        )}
      </div>
    </Modal>
  );
}
