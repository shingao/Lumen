import { CloseSmallIcon } from "../../components/icons";
import { useT } from "../../i18n";
import styles from "./HighlightChip.module.css";

interface Props {
  label: string;
  /** La mise en évidence concerne-t-elle une tuile de la vue ? */
  present: boolean;
  onClear(): void;
}

/** « Mise en évidence : Cache pip ✕ » : signale une mise en évidence persistante (panneau de
 * détail, « ? » d'un statut) ; ✕ l'arrête. */
export function HighlightChip({ label, present, onClear }: Props) {
  const t = useT();
  return (
    <div className={styles.chip} role="status" data-absent={!present || undefined}>
      <span className={styles.text}>
        {t("highlight.label", { name: label })}
        {!present && <span className={styles.absent}>{t("highlight.none")}</span>}
      </span>
      <button type="button" className={styles.clear} aria-label={t("highlight.clear", { name: label })} title={t("highlight.clear", { name: label })} onClick={onClear}>
        <CloseSmallIcon />
      </button>
    </div>
  );
}
