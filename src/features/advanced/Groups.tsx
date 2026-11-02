// Groupes repliables du Nettoyage (Dormants, Doublons, Gros fichiers) : une ligne principale
// repliée par défaut (flèche, case du groupe, chemin, nombre de fichiers, date la plus récente,
// taille totale) ; un clic sur la flèche ou la ligne déplie la liste. Clavier : ← / → replie ou
// déplie, Espace coche le groupe.

import { useId, useRef, type KeyboardEvent, type ReactNode } from "react";
import { setAllGroups, toggleGroup, useAdvanced, type GroupTab } from "../../app/advanced";
import { Button } from "../../components/Button";
import { formatBytes, formatCount, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import { Check } from "./FileRows";
import styles from "./Advanced.module.css";

export interface GroupCheck {
  checked: boolean;
  partial: boolean;
  onChange(on: boolean): void;
  label: string;
}

interface Props {
  tab: GroupTab;
  id: string;
  /** Chemin du dossier (ou nom du fichier pour un groupe de doublons). */
  title: string;
  /** Détail affiché à la place du nombre de fichiers (doublons : « 3 exemplaires de 1,2 Go »). */
  meta?: string;
  count: number;
  size: number;
  /** Date la plus récente du groupe (ms) ; 0 : inconnue. */
  latest: number;
  check: GroupCheck;
  extra?: ReactNode;
  children: ReactNode;
}

export function CollapsibleGroup({ tab, id, title, meta, count, size, latest, check, extra, children }: Props) {
  const t = useT();
  const open = useAdvanced((s) => s.expanded[tab].has(id));
  const listId = useId();
  const toggleRef = useRef<HTMLButtonElement>(null);
  const label = t(open ? "adv.collapse" : "adv.expand", { name: title });

  const onKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowRight") {
      e.preventDefault();
      toggleGroup(tab, id, true);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      toggleGroup(tab, id, false);
    } else if (e.key === " ") {
      e.preventDefault();
      check.onChange(!check.checked);
    }
  };
  // Depuis un fichier de la liste : ← replie le groupe et revient sur sa ligne principale.
  const onRowsKey = (e: KeyboardEvent<HTMLUListElement>) => {
    if (e.key !== "ArrowLeft") return;
    e.preventDefault();
    toggleGroup(tab, id, false);
    toggleRef.current?.focus();
  };

  return (
    <section className={styles.group} aria-label={title} data-open={open || undefined}>
      {/* Toute la ligne déplie ; la case et les boutons gardent leur propre clic. */}
      <div className={styles.groupHead} onClick={() => toggleGroup(tab, id)}>
        <button
          ref={toggleRef}
          type="button"
          className={styles.groupToggle}
          aria-expanded={open}
          aria-controls={listId}
          aria-label={label}
          title={label}
          onClick={(e) => {
            e.stopPropagation();
            toggleGroup(tab, id);
          }}
          onKeyDown={onKey}
        >
          <span aria-hidden="true" className={styles.arrow}>
            {open ? "▾" : "▸"}
          </span>
        </button>
        <span className={styles.groupCheck} onClick={(e) => e.stopPropagation()}>
          <Check checked={check.checked} partial={check.partial} label={check.label} onChange={check.onChange} />
        </span>
        <code className={styles.groupPath}>{title}</code>
        <span className={styles.groupMeta}>{meta ?? t.plural("adv.files", count, { n: formatCount(count, t.locale) })}</span>
        <span className={styles.groupMeta}>{latest > 0 ? t("adv.latest", { when: formatRelative(latest, t.locale) }) : "—"}</span>
        {extra && (
          <span className={styles.groupExtra} onClick={(e) => e.stopPropagation()}>
            {extra}
          </span>
        )}
        <span className={styles.groupSize}>{formatBytes(size, t.locale)}</span>
      </div>
      {open && (
        <ul id={listId} className={styles.rows} onKeyDown={onRowsKey}>
          {children}
        </ul>
      )}
    </section>
  );
}

/** « Tout déplier / Tout replier », en haut de la liste. */
export function GroupsToolbar({ tab, keys }: { tab: GroupTab; keys: readonly string[] }) {
  const t = useT();
  const expanded = useAdvanced((s) => s.expanded[tab]);
  const all = keys.length > 0 && keys.every((k) => expanded.has(k));
  return (
    <div className={styles.groupsBar}>
      <Button variant="link" onClick={() => setAllGroups(tab, keys, !all)} disabled={keys.length === 0}>
        {all ? t("adv.collapseAll") : t("adv.expandAll")}
      </Button>
    </div>
  );
}
