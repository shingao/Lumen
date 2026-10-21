import { useEffect, useRef, type ReactNode } from "react";
import { Button } from "../../components/Button";
import { formatBytes, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import type { FileRow } from "../../ipc/types";
import styles from "./Advanced.module.css";

/** Case à cocher qui sait afficher l'état « en partie » (dossier dont certains fichiers sont cochés). */
export function Check({ checked, partial, disabled, label, title, onChange }: { checked: boolean; partial?: boolean; disabled?: boolean; label: string; title?: string; onChange(on: boolean): void }) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = !checked && !!partial;
  }, [checked, partial]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className={styles.check}
      checked={checked}
      disabled={disabled}
      aria-label={label}
      title={title}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

/** Ligne d'un fichier proposé : nom, chemin, date, taille. `date` : dernier accès ou modification. */
export function FileLine({ file, checked, onChange, date = "modified", disabled, disabledTitle, badge }: {
  file: FileRow;
  checked: boolean;
  onChange(on: boolean): void;
  date?: "modified" | "accessed";
  disabled?: boolean;
  disabledTitle?: string;
  badge?: ReactNode;
}) {
  const t = useT();
  const when = date === "accessed" ? Math.max(file.accessed, file.modified) : file.modified;
  return (
    <li className={styles.row} data-checked={checked || undefined}>
      <label className={styles.rowLabel} title={disabled ? disabledTitle : undefined}>
        <Check checked={checked} disabled={disabled} label={t("adv.check", { name: file.name })} title={disabled ? disabledTitle : undefined} onChange={onChange} />
        <span className={styles.rowText}>
          <span className={styles.rowName}>
            {file.name}
            {badge}
          </span>
          <span className={styles.rowPath}>{file.path}</span>
        </span>
        <span className={styles.rowMeta}>
          {when > 0 ? t(date === "accessed" ? "adv.accessed" : "adv.modified", { when: formatRelative(when, t.locale) }) : "—"}
          {file.app && <span className={styles.tag}>{t("adv.app")}</span>}
        </span>
        <span className={styles.rowSize}>{formatBytes(file.size, t.locale)}</span>
      </label>
    </li>
  );
}

/** Pied de l'onglet : sélection et bouton « Nettoyer ». */
export function SelectionBar({ summary, onClear, action, disabled, onAction, extra }: {
  summary: string;
  onClear?: () => void;
  action: string;
  disabled: boolean;
  onAction(): void;
  extra?: ReactNode;
}) {
  const t = useT();
  return (
    <div className={styles.bar}>
      <span className={styles.barSummary}>{summary}</span>
      <div className={styles.barActions}>
        {extra}
        {onClear && (
          <Button variant="link" onClick={onClear} disabled={disabled}>
            {t("adv.clear")}
          </Button>
        )}
        <Button variant="clean" disabled={disabled} onClick={onAction}>
          {action}
        </Button>
      </div>
    </div>
  );
}
