import { ArrowLeftIcon, ArrowRightIcon } from "../../components/icons";
import { useT } from "../../i18n";
import styles from "./Explorer.module.css";

interface Props {
  /** Libellé de l'emplacement précédent / suivant (null : bouton grisé). */
  back: string | null;
  forward: string | null;
  onBack(): void;
  onForward(): void;
}

/** Précédent / suivant, comme un navigateur : Explorer et onglet Profondeur (même composant,
 * mêmes raccourcis Alt+← / Alt+→, Retour arrière, boutons 4 / 5 de la souris). */
export function HistoryButtons({ back, forward, onBack, onForward }: Props) {
  const t = useT();
  const backLabel = back !== null ? t("explorer.back.to", { name: back }) : t("explorer.back");
  const forwardLabel = forward !== null ? t("explorer.forward.to", { name: forward }) : t("explorer.forward");
  return (
    <div className={styles.history}>
      <button type="button" className={styles.historyButton} disabled={back === null} aria-label={backLabel} title={backLabel} onClick={onBack}>
        <ArrowLeftIcon />
      </button>
      <button type="button" className={styles.historyButton} disabled={forward === null} aria-label={forwardLabel} title={forwardLabel} onClick={onForward}>
        <ArrowRightIcon />
      </button>
    </div>
  );
}
