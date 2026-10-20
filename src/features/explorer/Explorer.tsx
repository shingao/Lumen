import { useMemo, useState } from "react";
import { copyText } from "../../app/clipboard";
import { useRecommendations, useTree, useVolumes } from "../../app/data";
import { type ColorMode, useStore } from "../../app/store";
import { ChevronDownIcon } from "../../components/icons";
import { Menu } from "../../components/Menu";
import { Segmented } from "../../components/Segmented";
import { formatBytes } from "../../core/format";
import { currentT, useT } from "../../i18n";
import type { Category, TreeNode } from "../../ipc/types";
import { Legend } from "./Legend";
import { Treemap } from "./Treemap";
import styles from "./Explorer.module.css";

function presentCategories(root: TreeNode): Set<Category> {
  const set = new Set<Category>();
  for (const c of root.children) {
    set.add(c.category);
    for (const g of c.children) set.add(g.category);
  }
  return set;
}

export function Explorer() {
  const t = useT();
  const volume = useStore((s) => s.volume);
  const node = useStore((s) => s.node);
  const zoom = useStore((s) => s.zoom);
  const setVolume = useStore((s) => s.setVolume);
  const colorMode = useStore((s) => s.colorMode);
  const setColorMode = useStore((s) => s.setColorMode);
  const hoverReco = useStore((s) => s.hoverReco);
  const openReco = useStore((s) => s.openReco);
  const showToast = useStore((s) => s.showToast);
  const tree = useTree(volume, node);
  const recos = useRecommendations(volume);
  const volumes = useVolumes();
  const [driveMenu, setDriveMenu] = useState<{ x: number; y: number } | null>(null);

  const focus = hoverReco ?? openReco;
  const highlight = useMemo(() => recos?.find((r) => r.id === focus)?.highlight ?? null, [recos, focus]);
  const categories = useMemo(() => (tree ? presentCategories(tree.root) : new Set<Category>()), [tree]);

  const modes: { value: ColorMode; label: string }[] = [
    { value: "category", label: t("explorer.mode.category") },
    { value: "cleanup", label: t("explorer.mode.cleanup") },
    { value: "age", label: t("explorer.mode.age") },
  ];

  const crumbs = tree?.breadcrumb ?? [];
  const [drive, ...rest] = crumbs;
  const rootCrumb = crumbs.find((c) => c.id !== null)?.id ?? null;
  const isUserRoot = volume === "C:";

  return (
    <section aria-label={t("explorer.title")} className={styles.explorer}>
      <div className={styles.header}>
        <div className={styles.heading}>
          <h2 className={styles.title}>{t("explorer.title")}</h2>
          <p className={styles.subtitle}>{isUserRoot ? t("explorer.subtitleUser") : t("explorer.subtitleVolume")}</p>
        </div>
        <Segmented options={modes} value={colorMode} onChange={setColorMode} label={t("explorer.coloring")} />
      </div>

      <div className={styles.crumbBar}>
        <nav aria-label={t("explorer.breadcrumb")} className={styles.crumbs}>
          <button
            type="button"
            className={styles.drive}
            aria-haspopup="menu"
            aria-expanded={driveMenu !== null}
            aria-label={t("explorer.drive", { drive: volume })}
            onClick={(e) => {
              const box = e.currentTarget.getBoundingClientRect();
              setDriveMenu({ x: box.left, y: box.bottom + 4 });
            }}
          >
            {drive?.label ?? volume}
            <ChevronDownIcon />
          </button>
          {rest.map((c, i) => {
            const last = i === rest.length - 1;
            return (
              <span key={`${c.label}-${i}`} className={styles.crumbItem}>
                <span aria-hidden="true" className={styles.sep}>
                  \
                </span>
                {c.id !== null && !last ? (
                  <button type="button" className={styles.crumbLink} onClick={() => zoom(c.id === rootCrumb ? null : c.id)}>
                    {c.label}
                  </button>
                ) : (
                  <span className={last ? styles.current : undefined} aria-current={last ? "location" : undefined}>
                    {c.label}
                  </span>
                )}
              </span>
            );
          })}
        </nav>
        <span className={styles.total}>{tree ? formatBytes(tree.root.size, t.locale) : ""}</span>
      </div>

      {tree && (
        <Treemap
          root={tree.root}
          highlight={highlight}
          onCopyPath={(n) => {
            void copyText(n.path).then((ok) => ok && showToast(currentT()("preview.pathCopied")));
          }}
          onUnavailable={(n) => showToast(currentT()("preview.later", { n }))}
        />
      )}

      <Legend mode={colorMode} categories={categories} />

      {driveMenu && volumes && (
        <Menu
          x={driveMenu.x}
          y={driveMenu.y}
          label={t("explorer.drives")}
          onClose={() => setDriveMenu(null)}
          items={volumes.map((v) => ({
            label: `${v.letter}  ${v.label}`,
            hint: t("explorer.driveFree", { free: formatBytes(v.free, t.locale), total: formatBytes(v.total, t.locale) }),
            disabled: v.kind === "network",
            onSelect: () => setVolume(v.id),
          }))}
        />
      )}
    </section>
  );
}
