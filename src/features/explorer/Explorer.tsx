import { useEffect, useMemo, useState } from "react";
import { copyText } from "../../app/clipboard";
import { useRecommendations, useTree, useVolumes } from "../../app/data";
import { goBack, goForward, goTo, setHere, useNav } from "../../app/navigation";
import { startScan } from "../../app/scanning";
import { type ColorMode, useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { ArrowLeftIcon, ArrowRightIcon } from "../../components/icons";
import { Segmented } from "../../components/Segmented";
import { openMove } from "../../app/moves";
import { openExport } from "../../app/report";
import { formatBytes } from "../../core/format";
import { currentT, useT } from "../../i18n";
import type { Category, TreeNode } from "../../ipc/types";
import { DeviceChips } from "./DeviceChips";
import { FastScanConfirm } from "./FastScanConfirm";
import { Legend } from "./Legend";
import { ScanProgressPanel, ScanStats } from "./ScanPanel";
import { KEEP_FOCUS } from "../status/StatusHelp";
import { activeSource, inView } from "../../core/highlight";
import { HighlightChip } from "./HighlightChip";
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
  const fastScanAvailable = useStore((s) => s.appInfo?.fastScanAvailable ?? false);
  const node = useStore((s) => s.node);
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  // Déplacement intelligent : réglage Expérimental tant qu'il n'est pas vérifié en VM.
  const moveOn = useStore((s) => s.settings?.experimentalMove ?? false);
  const back = useNav((s) => s.back.at(-1) ?? null);
  const forward = useNav((s) => s.forward.at(-1) ?? null);
  const colorMode = useStore((s) => s.colorMode);
  const setColorMode = useStore((s) => s.setColorMode);
  const hoverReco = useStore((s) => s.hoverReco);
  const statusFocus = useStore((s) => s.statusFocus);
  const openReco = useStore((s) => s.openReco);
  const showToast = useStore((s) => s.showToast);
  const tree = useTree(node);
  const recos = useRecommendations();
  const volumes = useVolumes();
  const [fastAnchor, setFastAnchor] = useState<{ x: number; y: number } | null>(null);

  // Emplacement affiché, pour l'historique (libellé et chemin du dossier, valables d'une analyse à l'autre).
  useEffect(() => {
    if (!tree) return;
    const label = tree.breadcrumb.at(-1)?.label ?? tree.root.name;
    setHere({ target, scanId, node, path: node === null ? null : tree.root.path, label });
  }, [tree, target, scanId, node]);

  // Une seule source d'estompage : survol d'une recommandation, sinon panneau de détail ouvert,
  // sinon « ? » d'un statut ; rien d'estompé si elle ne concerne aucune tuile de la vue.
  const source = useMemo(() => activeSource(hoverReco, openReco, statusFocus?.status ?? null, recos ?? []), [hoverReco, openReco, statusFocus, recos]);
  const view = useMemo(
    () => (tree ? inView(source, tree.root, tree.breadcrumb.flatMap((c) => (c.id === null ? [] : [c.id]))) : { keep: null, present: false }),
    [source, tree],
  );
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
  // Scan rapide : disque fixe NTFS, Lumen installé dans Program Files (assistant élevé vérifié), et
  // seulement si l'appli n'est pas déjà administrateur (sinon la MFT est lue d'office).
  const canFastScan = !elevated && fastScanAvailable && (targetVolume?.mftCapable ?? false);

  // Clé USB, carte SD, lecteur réseau : analyse et export seulement (pas de Corbeille).
  const noCleanup = target.kind === "volume" && targetVolume !== undefined && !targetVolume.cleanable;

  return (
    <section aria-label={t("explorer.title")} className={styles.explorer} {...{ [KEEP_FOCUS]: "" }}>
      <div className={styles.header}>
        <div className={styles.heading}>
          <h2 className={styles.title}>{t("explorer.title")}</h2>
          <p className={styles.subtitle}>{isUserRoot ? t("explorer.subtitleUser") : t("explorer.subtitleVolume")}</p>
        </div>
        <Segmented options={modes} value={colorMode} onChange={setColorMode} label={t("explorer.coloring")} />
      </div>

      <DeviceChips />
      {noCleanup && (
        <p className={styles.banner} role="note">
          {targetVolume.kind === "network" ? t("devices.bannerNetwork") : t("devices.bannerRemovable")}
        </p>
      )}

      <div className={styles.crumbBar}>
        <div className={styles.history}>
          <button
            type="button"
            className={styles.historyButton}
            disabled={!back}
            aria-label={back ? t("explorer.back.to", { name: back.label }) : t("explorer.back")}
            title={back ? t("explorer.back.to", { name: back.label }) : t("explorer.back")}
            onClick={goBack}
          >
            <ArrowLeftIcon />
          </button>
          <button
            type="button"
            className={styles.historyButton}
            disabled={!forward}
            aria-label={forward ? t("explorer.forward.to", { name: forward.label }) : t("explorer.forward")}
            title={forward ? t("explorer.forward.to", { name: forward.label }) : t("explorer.forward")}
            onClick={goForward}
          >
            <ArrowRightIcon />
          </button>
        </div>
        <nav aria-label={t("explorer.breadcrumb")} className={styles.crumbs}>
          <button type="button" className={styles.drive} aria-label={t("explorer.drive", { drive: targetLetter })} onClick={() => goTo(null)}>
            {drive?.label ?? targetLetter}
          </button>
          {rest.map((c, i) => {
            const last = i === rest.length - 1;
            return (
              <span key={`${c.label}-${i}`} className={styles.crumbItem}>
                <span aria-hidden="true" className={styles.sep}>
                  \
                </span>
                {c.id !== null && !last ? (
                  <button type="button" className={styles.crumbLink} onClick={() => goTo(c.id === rootCrumb ? null : c.id)}>
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
              {moveOn && node !== null && scanId !== null && tree?.root.kind === "dir" && (
                <Button variant="link" className={styles.scanLink} title={t("move.openLabel")} aria-haspopup="dialog" onClick={() => openMove(scanId, node)}>
                  {t("move.open")}
                </Button>
              )}
              {scan.summary && (
                <Button variant="link" className={styles.scanLink} aria-haspopup="dialog" onClick={openExport}>
                  {t("export.open")}
                </Button>
              )}
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

      {source && source.kind !== "hover" && (
        <HighlightChip
          label={source.kind === "panel" ? source.label : t(`status.${source.status}`)}
          present={view.present}
          onClear={() => (source.kind === "panel" ? useStore.getState().setOpenReco(null) : useStore.getState().setStatusFocus(null))}
        />
      )}

      <div className={styles.mapArea}>
        {tree && (
          <Treemap
            root={tree.root}
            highlight={view.keep ? view.keep.ids : null}
            highlightStatus={view.keep?.status ?? null}
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
