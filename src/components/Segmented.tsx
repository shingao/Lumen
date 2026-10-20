import styles from "./Segmented.module.css";

export interface SegmentOption<T extends string | number> {
  value: T;
  label: string;
}

interface Props<T extends string | number> {
  options: readonly SegmentOption<T>[];
  value: T;
  onChange(value: T): void;
  label: string;
  /** « nav » : navigation principale (aria-current), sinon groupe de bascules (aria-pressed). */
  as?: "group" | "nav";
}

/** Contrôle segmenté textuel de la maquette (Catégorie / Nettoyage). */
export function Segmented<T extends string | number>({ options, value, onChange, label, as = "group" }: Props<T>) {
  const items = options.map((o) => {
    const on = o.value === value;
    return (
      <button
        key={String(o.value)}
        type="button"
        className={styles.item}
        data-on={on || undefined}
        aria-pressed={as === "group" ? on : undefined}
        aria-current={as === "nav" && on ? "page" : undefined}
        onClick={() => onChange(o.value)}
      >
        {o.label}
      </button>
    );
  });
  if (as === "nav") {
    return (
      <nav aria-label={label} className={styles.root}>
        {items}
      </nav>
    );
  }
  return (
    <div role="group" aria-label={label} className={styles.root}>
      {items}
    </div>
  );
}
