import type { ReactNode } from "react";
import { useT, type MessageKey } from "../../i18n";
import styles from "./Screens.module.css";

interface Props {
  title: MessageKey;
  subtitle: MessageKey;
  empty: MessageKey;
  milestone: number;
  /** Contrôles d'en-tête (onglets, filtres), à droite du titre. */
  tools?: ReactNode;
  children?: ReactNode;
}

/** Écran à venir : même conteneur, même en-tête que l'Explorer, état vide en attendant son jalon. */
export function ScreenStub({ title, subtitle, empty, milestone, tools, children }: Props) {
  const t = useT();
  return (
    <section aria-label={t(title)} className={`${styles.panel} ${styles.stub}`}>
      <div className={styles.header}>
        <div className={styles.heading}>
          <h2 className={styles.title}>{t(title)}</h2>
          <p className={styles.subtitle}>{t(subtitle)}</p>
        </div>
        {tools}
      </div>
      {children}
      <div className={styles.empty}>
        <p>{t(empty)}</p>
        <span className={styles.milestone}>{t("screen.milestone", { n: milestone })}</span>
      </div>
    </section>
  );
}
