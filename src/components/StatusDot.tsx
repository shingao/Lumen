import type { Status } from "../ipc/types";
import styles from "./StatusDot.module.css";

/** Pastille de statut ; « outline » : le cercle vide de « Non analysé » ; « protected » : zone de
 * la denylist (écran Profondeur). */
export function StatusDot({ status }: { status: Status | "outline" | "protected" }) {
  return <span aria-hidden="true" className={styles.dot} data-status={status} />;
}
