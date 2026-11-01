import { useEffect, useState } from "react";
import { useCopyFlash } from "../../app/clipboard";
import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { useT } from "../../i18n";
import summary from "../../assets/notices-summary.json";
import pkg from "../../../package.json";
import styles from "./SettingsScreen.module.css";

/** Lien affiché et copié, jamais ouvert : Lumen n'accède pas au réseau. */
export const REPOSITORY = "https://github.com/shingao/Lumen";

/** Licences des composants tiers : THIRD_PARTY_NOTICES.txt (généré par `npm run notices`), chargé à la demande. */
function Notices({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    void import("../../../THIRD_PARTY_NOTICES.txt?raw").then((m) => setText(m.default));
  }, []);
  return (
    <Modal title={t("about.notices")} onClose={onClose}>
      <pre className={styles.notices} tabIndex={0} aria-label={t("about.notices")}>
        {text ?? ""}
      </pre>
      <div className={styles.modalActions}>
        <Button variant="primary" onClick={onClose}>
          {t("about.noticesClose")}
        </Button>
      </div>
    </Modal>
  );
}

/** À propos : version (sans mise à jour automatique), licence, dépôt, composants tiers. */
export function AboutGroup() {
  const t = useT();
  const [copied, copy] = useCopyFlash();
  const [notices, setNotices] = useState(false);
  return (
    <section className={styles.group} aria-label={t("settings.about")}>
      <h3 className={styles.groupTitle}>{t("settings.about")}</h3>
      <div className={styles.rows}>
        <div className={styles.row}>
          <div className={styles.rowText}>
            <span className={styles.rowLabel}>{t("about.version")}</span>
            <span className={styles.rowHint}>{t("about.versionHint")}</span>
          </div>
          <div className={styles.rowControl}>
            <span className={`${styles.value} ${styles.mono}`}>{pkg.version}</span>
          </div>
        </div>
        <div className={styles.row}>
          <div className={styles.rowText}>
            <span className={styles.rowLabel}>{t("about.license")}</span>
            <span className={styles.rowHint}>{t("about.licenseHint")}</span>
          </div>
          <div className={styles.rowControl}>
            <span className={styles.value}>{t("about.licenseValue")}</span>
          </div>
        </div>
        <div className={styles.row}>
          <div className={styles.rowText}>
            <span className={styles.rowLabel}>{t("about.repo")}</span>
            <span className={`${styles.rowHint} ${styles.mono}`}>{REPOSITORY}</span>
          </div>
          <div className={styles.rowControl}>
            <Button variant="link" onClick={() => void copy(REPOSITORY)}>
              {copied ? t("about.repoCopied") : t("about.repoCopy")}
            </Button>
          </div>
        </div>
        <div className={styles.row}>
          <div className={styles.rowText}>
            <span className={styles.rowLabel}>{t("about.notices")}</span>
            <span className={styles.rowHint}>{t("about.noticesHint", { rust: summary.rust, npm: summary.npm })}</span>
          </div>
          <div className={styles.rowControl}>
            <Button variant="link" onClick={() => setNotices(true)}>
              {t("about.noticesOpen")}
            </Button>
          </div>
        </div>
      </div>
      {notices && <Notices onClose={() => setNotices(false)} />}
    </section>
  );
}
