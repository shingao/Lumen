import { useMemo, useState } from "react";
import { copyText } from "../../app/clipboard";
import { useRecommendations, useTree, useVolumes } from "../../app/data";
import { startScan } from "../../app/scanning";
import { type ColorMode, useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { ChevronDownIcon } from "../../components/icons";
import { Menu, type MenuItem } from "../../components/Menu";
import { Segmented } from "../../components/Segmented";
import { formatBytes } from "../../core/format";
import { currentT, useT } from "../../i18n";
import type { Category, TreeNode } from "../../ipc/types";
import { FastScanConfirm } from "./FastScanConfirm";
import { Legend } from "./Legend";
import { ScanProgressPanel, ScanStats } from "./ScanPanel";
import { Treemap } from "./Treemap";
import styles from "./Explorer.module.css";

function presentCategories(root: TreeNode): Set<Category> {
  const set = new Set<Category>();
  for (const c of root.children) {
    if (c.othersCount !== null) continue;
    set.add(c.category);
    for (const g of c.children) if (g.othersCount === null) set.add(g.category);
  }
  return set;
}

export function Explorer() {
  const t = useT();
  const target = useStore((s) => s.target);
  const scan = useStore((s) => s.scan);
  const elevated = useStore((s) => s.appInfo?.elevated ?? false);
  const node = useStore((s) => s.node);
  const zoom = useStore((s) => s.zoom);
  const colorMode = useStore((s) => s.colorMode);
  const setColorMode = useStore((s) => s.setColorMode);
  const hoverReco = useStore((s) => s.hoverReco);
  const openReco = useStore((s) => s.openReco);
  const showToast = useStore((s) => s.showToast);
  const tree = useTree(node);
  const recos = useRecommendations();
  const volumes = useVolumes();
  const [driveMenu, setDriveMenu] = useState<{ x: number; y: number } | null>(null);
  const [fastAnchor, setFastAnchor] = useState<{ x: number; y: number } | null>(null);

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
  const isUserRoot = target.kind === "userProfile";
  const running = scan.status === "running";
  const targetLetter = target.kind === "volume" ? target.letter : (scan.summary?.volume ?? "C:");
  const targetVolume = volumes?.find((v) => v.letter.toUpperCase() === targetLetter.toUpperCase());
  // Scan rapide : NTFS local, et seulement si l'appli n'est pas déjà administrateur (sinon automatique).
  const canFastScan = !elevated && (targetVolume?.mftCapable ?? false);

  const targetItems: MenuItem[] = [
    { label: t("explorer.target.user"), onSelect: () => void startScan({ kind: "userProfile" }) },
    ...(volumes ?? []).map((v, i) => ({
      label: t("explorer.target.volume", { letter: v.letter, label: v.label || t(`explorer.volumeKind.${v.kind}`) }),
      hint: t("explorer.driveFree", { free: formatBytes(v.free, t.locale), total: formatBytes(v.total, t.locale) }),
      separatorBefore: i === 0,
      onSelect: () => void startScan({ kind: "volume", letter: v.letter }),
    })),
  ];

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
            aria-label={t("explorer.drive", { drive: targetLetter })}
            onClick={(e) => {
              const box = e.currentTarget.getBoundingClientRect();
              setDriveMenu({ x: box.left, y: box.bottom + 4 });
            }}
          >
            {drive?.label ?? targetLetter}
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
        <div className={styles.crumbEnd}>
          {!running && (
            <>
              <Button variant="link" className={styles.scanLink} onClick={() => void startScan(target)}>
                {t("explorer.rescan")}
              </Button>
              {canFastScan && (
                <Button
                  variant="link"
                  className={styles.scanLink}
                  aria-haspopup="dialog"
                  onClick={(e) => {
                    const box = e.currentTarget.getBoundingClientRect();
                    setFastAnchor({ x: box.right, y: box.bottom + 6 });
                  }}
                >
                  {t("explorer.fastScan")}
                </Button>
              )}
            </>
          )}
          <span className={styles.total}>{tree ? formatBytes(tree.root.size, t.locale) : ""}</span>
        </div>
      </div>

      <div className={styles.mapArea}>
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
        {(running || !tree) && <ScanProgressPanel overlay={Boolean(tree)} />}
      </div>

      <Legend mode={colorMode} categories={categories} />
      <ScanStats />

      {driveMenu && <Menu x={driveMenu.x} y={driveMenu.y} label={t("explorer.targets")} onClose={() => setDriveMenu(null)} items={targetItems} />}
      {fastAnchor && (
        <FastScanConfirm
          x={fastAnchor.x}
          y={fastAnchor.y}
          onCancel={() => setFastAnchor(null)}
          onConfirm={() => {
            setFastAnchor(null);
            void startScan(target, "fast");
          }}
        />
      )}
    </section>
  );
}
