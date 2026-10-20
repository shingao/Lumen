import type { CSSProperties } from "react";
import type { ColorMode } from "../../app/store";
import { useT } from "../../i18n";
import type { Category } from "../../ipc/types";
import { CATEGORY_ORDER, categoryFill } from "./tileFill";
import styles from "./Legend.module.css";

interface Item {
  label: string;
  bg: string;
  hatch?: boolean;
}

function Swatch({ item }: { item: Item }) {
  return (
    <span className={styles.item}>
      <span aria-hidden="true" className={styles.swatch} data-hatch={item.hatch || undefined} style={{ "--tile-bg": item.bg } as CSSProperties} />
      {item.label}
    </span>
  );
}

export function Legend({ mode, categories }: { mode: ColorMode; categories: ReadonlySet<Category> }) {
  const t = useT();
  let items: Item[];
  let note: string | null = null;
  if (mode === "category") {
    items = CATEGORY_ORDER.filter((c) => categories.has(c)).map((c) => ({ label: t(`category.${c}`), bg: categoryFill(c) }));
    items.push({ label: t("legend.hatch"), bg: "var(--bg-container)", hatch: true });
  } else if (mode === "cleanup") {
    items = [
      { label: t("status.safe"), bg: "var(--st-safe)", hatch: true },
      { label: t("status.review"), bg: "var(--st-review)" },
      { label: t("status.keep"), bg: "var(--st-keep)" },
    ];
  } else {
    items = ([0, 1, 2, 3, 4] as const).map((i) => ({ label: t(`legend.age${i}`), bg: `var(--age-${i})` }));
    note = t("legend.ageNote");
  }
  return (
    <div aria-label={t("legend.label")} className={styles.legend}>
      {items.map((item) => (
        <Swatch key={item.label} item={item} />
      ))}
      {note && <span className={styles.note}>{note}</span>}
    </div>
  );
}
