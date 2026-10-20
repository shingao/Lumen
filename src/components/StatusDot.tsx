import type { Status } from "../ipc/types";
import styles from "./StatusDot.module.css";

/** Pastille de statut ; « outline » : le cercle vide de « Non analysé ». */
export function StatusDot({ status }: { status: Status | "outline" }) {
  return <span aria-hidden="true" className={styles.dot} data-status={status} />;
}
