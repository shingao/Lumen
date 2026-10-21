import { type CSSProperties, useCallback, useMemo, useState } from "react";
import { goTo } from "../../app/navigation";
import { useStore } from "../../app/store";
import { useSize } from "../../app/useSize";
import { Menu, type MenuItem } from "../../components/Menu";
import { StatusDot } from "../../components/StatusDot";
import { ExpandIcon } from "../../components/icons";
import { formatBytes, formatCount, formatDelta, formatRelative } from "../../core/format";
import { nests, SIZE_MIN_W } from "../../core/treemap/depth";
import { squarify } from "../../core/treemap/squarify";
import { type Translator, useT } from "../../i18n";
import type { NodeId, TreeNode } from "../../ipc/types";
import pop from "../../components/Popover.module.css";
import { tileFill } from "./tileFill";
import styles from "./Treemap.module.css";

const GAP = 3;
const TOOLTIP_W = 268;
// Place du reste du dossier (jamais dessinée) ; les regroupements « autres » ont des id négatifs.
const PHANTOM = Number.MIN_SAFE_INTEGER;

interface Props {
  root: TreeNode;
  /** Tuiles mises en avant (survol ou détail d'une recommandation) ; null = aucune. */
  highlight: readonly NodeId[] | null;
  onCopyPath(node: TreeNode): void;
  onUnavailable(milestone: number): void;
}

const area = (n: TreeNode) => n.layoutSize ?? n.size;

/** Les sous-tuiles plus la place du reste du dossier (non dessinée), comme la maquette. */
function childItems(node: TreeNode): TreeNode[] {
  const used = node.children.reduce((s, c) => s + area(c), 0);
  const rest = area(node) - used;
  if (rest <= area(node) * 0.002) return node.children;
  return [...node.children, { ...node, id: PHANTOM, layoutSize: rest, children: [] }];
}

function deltaText(node: TreeNode, t: Translator): string | null {
  if (node.gain <= 0 || !node.gainStatus) return null;
  const delta = formatDelta(-node.gain, t.locale);
  if (node.gainUpTo) return t("explorer.upTo", { delta });
  return node.gainStatus === "review" ? `${delta} ?` : delta;
}

/** Nom affiché : celui de l'élément, ou « 12 autres éléments » pour un regroupement. */
function label(node: TreeNode, t: Translator): string {
  return node.othersCount !== null ? t.plural("explorer.others", node.othersCount, { n: node.othersCount }) : node.name;
}

/** Opacité de la maquette : estompe tout sauf les tuiles concernées (et l'intérieur des parents estompés). */
function isDimmed(id: NodeId, parent: NodeId | null, keep: readonly NodeId[] | null): boolean {
  if (!keep) return false;
  if (keep.includes(id)) return false;
  if (parent !== null && !keep.includes(parent)) return false;
  return true;
}

/** Toutes les tuiles de la tranche, à tous les niveaux, avec leur parent. */
function indexOf(root: TreeNode): Map<NodeId, { node: TreeNode; parent: NodeId | null }> {
  const map = new Map<NodeId, { node: TreeNode; parent: NodeId | null }>();
  const walk = (n: TreeNode, parent: NodeId | null) => {
    map.set(n.id, { node: n, parent });
    for (const c of n.children) walk(c, n.id);
  };
  for (const c of root.children) walk(c, null);
  return map;
}

export function Treemap({ root, highlight, onCopyPath, onUnavailable }: Props) {
  const t = useT();
  const mode = useStore((s) => s.colorMode);
  const setOpenReco = useStore((s) => s.setOpenReco);
  const checked = useStore((s) => s.checked);
  const toggleChecked = useStore((s) => s.toggleChecked);
  const [ref, size] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<{ id: NodeId; x: number; y: number } | null>(null);
  const [menu, setMenu] = useState<{ node: TreeNode; x: number; y: number } | null>(null);
  const [now] = useState(() => Date.now());

  const index = useMemo(() => indexOf(root), [root]);

  const tiles = useMemo(
    () => squarify(root.children, area, { x: 0, y: 0, w: size.width, h: size.height }, GAP),
    [root, size.width, size.height],
  );

  const activate = useCallback(
    (node: TreeNode) => {
      if (node.hasChildren) goTo(node.id);
      else if (node.reco) setOpenReco(node.reco);
    },
    [setOpenReco],
  );

  const openMenu = (node: TreeNode, x: number, y: number) => {
    setHover(null);
    setMenu({ node, x, y });
  };

  const menuItems = (node: TreeNode): MenuItem[] => {
    const milestone = (n: number) => t("screen.milestone", { n });
    const reco = node.reco;
    return [
      { label: t("menu.open"), hint: milestone(3), disabled: true, onSelect: () => onUnavailable(3) },
      { label: t("menu.reveal"), hint: milestone(3), disabled: true, onSelect: () => onUnavailable(3) },
      {
        label: t("menu.select"),
        disabled: !reco || checked.includes(reco),
        onSelect: () => reco && toggleChecked(reco),
      },
      { label: t("menu.move"), hint: milestone(9), disabled: true, onSelect: () => onUnavailable(9) },
      { label: t("menu.copyPath"), separatorBefore: true, onSelect: () => onCopyPath(node) },
    ];
  };

  const hovered = hover ? index.get(hover.id)?.node : undefined;

  return (
    <div
      ref={ref}
      className={styles.map}
      role="group"
      aria-label={t("explorer.treemap")}
      onPointerMove={(e) => {
        const el = (e.target as HTMLElement).closest<HTMLElement>("[data-node]");
        const id = el ? Number(el.dataset.node) : null;
        const box = e.currentTarget.getBoundingClientRect();
        setHover(id === null || id === PHANTOM ? null : { id, x: e.clientX - box.left, y: e.clientY - box.top });
      }}
      onPointerLeave={() => setHover(null)}
    >
      <div key={root.id} className={styles.layer}>
        {tiles.map(({ item, rect }) => (
          <Tile
            key={item.id}
            node={item}
            style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}
            width={rect.w}
            height={rect.h}
            hoverId={hover?.id ?? null}
            highlight={highlight}
            now={now}
            mode={mode}
            t={t}
            onActivate={activate}
            onMenu={openMenu}
          />
        ))}
        {root.children.length === 0 && <p className={styles.empty}>{t("explorer.empty")}</p>}
      </div>

      {hover && hovered && !menu && (
        <TileTooltip node={hovered} x={hover.x} y={hover.y} width={size.width} height={size.height} now={now} t={t} />
      )}

      {menu && (
        <Menu
          x={menu.x}
          y={menu.y}
          label={t("menu.label", { name: label(menu.node, t) })}
          items={menuItems(menu.node)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

interface TileProps {
  node: TreeNode;
  style: CSSProperties;
  width: number;
  height: number;
  hoverId: NodeId | null;
  highlight: readonly NodeId[] | null;
  now: number;
  mode: ReturnType<typeof useStore.getState>["colorMode"];
  t: Translator;
  onActivate(node: TreeNode): void;
  onMenu(node: TreeNode, x: number, y: number): void;
}

/** « Étendre » : zoom direct sur ce dossier (au survol ; au clavier, Entrée sur la tuile). */
function Expand({ node, t }: { node: TreeNode; t: Translator }) {
  if (!node.hasChildren || node.othersCount !== null) return null;
  const name = t("explorer.expand", { name: label(node, t) });
  return (
    <span
      aria-hidden="true"
      className={styles.expand}
      title={name}
      data-expand={node.id}
      onClick={(e) => {
        e.stopPropagation();
        goTo(node.id);
      }}
    >
      <ExpandIcon />
    </span>
  );
}

function Tile({ node, style, width, height, hoverId, highlight, now, mode, t, onActivate, onMenu }: TileProps) {
  const fill = tileFill(node, mode, now);
  const delta = deltaText(node, t);
  const small = width < 96 || height < 64;
  const size = formatBytes(node.size, t.locale);
  // Hauteurs sur une ligne (en-tête, taille) : la taille en bas n'est jamais rognée.
  const showHead = width >= 36 && height >= 22;
  const showFoot = width >= 36 && height >= (small ? 44 : 52);
  return (
    <div
      data-node={node.id}
      className={styles.tile}
      data-small={small || undefined}
      data-hatch={fill.hatch || undefined}
      data-deep={fill.deep || undefined}
      data-hover={hoverId === node.id || undefined}
      data-dim={isDimmed(node.id, null, highlight) || undefined}
      data-clickable={node.hasChildren || node.reco !== null || undefined}
      style={{ ...style, "--tile-bg": fill.bg } as CSSProperties}
      tabIndex={0}
      role="button"
      aria-label={t("explorer.tileLabel", { name: label(node, t), size })}
      onClick={() => onActivate(node)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onActivate(node);
        } else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
          e.preventDefault();
          const box = e.currentTarget.getBoundingClientRect();
          onMenu(node, box.left + 12, box.top + 24);
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onMenu(node, e.clientX, e.clientY);
      }}
    >
      {width >= 40 && height >= 30 && <Expand node={node} t={t} />}
      {showHead && (
        <div className={styles.head}>
          <span className={styles.name}>{label(node, t)}</span>
          {delta && (
            <span className={styles.delta} data-status={node.gainStatus}>
              {delta}
            </span>
          )}
        </div>
      )}
      {node.children.length > 0 && height >= 72 && width >= 48 && (
        <Children node={node} level={1} hoverId={hoverId} highlight={highlight} now={now} mode={mode} t={t} onActivate={onActivate} onMenu={onMenu} />
      )}
      {showFoot && <span className={styles.foot}>{size}</span>}
    </div>
  );
}

type ChildrenProps = Omit<TileProps, "style" | "width" | "height"> & {
  /** Niveau de `node` (1 : tuile du premier niveau). */
  level: number;
};

function Children({ node, level, hoverId, highlight, now, mode, t, onActivate, onMenu }: ChildrenProps) {
  const [ref, size] = useSize<HTMLDivElement>();
  const placed = useMemo(
    () => squarify(childItems(node), area, { x: 0, y: 0, w: size.width, h: size.height }, GAP),
    [node, size.width, size.height],
  );
  return (
    <div ref={ref} className={styles.children}>
      {placed.map(({ item, rect }) => {
        if (item.id === PHANTOM) return null;
        const fill = tileFill(item, mode, now);
        const delta = deltaText(item, t);
        const showLabel = rect.w >= 34 && rect.h >= 18;
        // Profondeur adaptative : une sous-tuile assez grande montre ses propres enfants.
        const group = nests(item.children.length, level + 1, rect.w, rect.h);
        return (
          <div
            key={item.id}
            data-node={item.id}
            className={styles.child}
            data-group={group || undefined}
            data-level={level + 1}
            data-tiny={rect.w < 24 || rect.h < 24 || undefined}
            data-hatch={fill.hatch || undefined}
            data-deep={fill.deep || undefined}
            data-hover={hoverId === item.id || undefined}
            data-dim={isDimmed(item.id, node.id, highlight) || undefined}
            style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, "--tile-bg": fill.bg } as CSSProperties}
            onClick={(e) => {
              if (!item.hasChildren && !item.reco) return;
              e.stopPropagation();
              onActivate(item);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onMenu(item, e.clientX, e.clientY);
            }}
          >
            {rect.w >= 40 && rect.h >= 24 && <Expand node={item} t={t} />}
            {showLabel && (
              <div className={styles.childHead}>
                <span className={styles.childName}>{label(item, t)}</span>
                {delta ? (
                  <span className={styles.delta} data-status={item.gainStatus}>
                    {delta}
                  </span>
                ) : (
                  rect.w >= SIZE_MIN_W && <span className={styles.childSize}>{formatBytes(item.size, t.locale)}</span>
                )}
              </div>
            )}
            {group && (
              <Children
                node={item}
                level={level + 1}
                hoverId={hoverId}
                highlight={highlight}
                now={now}
                mode={mode}
                t={t}
                onActivate={onActivate}
                onMenu={onMenu}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

interface TooltipProps {
  node: TreeNode;
  x: number;
  y: number;
  width: number;
  height: number;
  now: number;
  t: Translator;
}

/** Infobulle de la maquette : chemin complet, taille, fichiers, dernière ouverture, statut. */
function TileTooltip({ node, x, y, width, height, now, t }: TooltipProps) {
  let left = x + 16;
  if (left + TOOLTIP_W + 4 > width) left = Math.max(0, x - TOOLTIP_W - 16);
  let top = y + 16;
  if (top + 150 > height) top = Math.max(0, y - 162);
  return (
    <div role="tooltip" className={`${pop.pop} ${styles.tooltip}`} style={{ left: Math.round(left), top: Math.round(top) }}>
      <span className={styles.tipPath}>{node.othersCount !== null ? `${node.path} · ${label(node, t)}` : node.path}</span>
      <div className={styles.tipGrid}>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.size")}</span>
          <span className={styles.tipMono}>{formatBytes(node.size, t.locale)}</span>
        </div>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.logical")}</span>
          <span className={styles.tipMono}>{formatBytes(node.logical, t.locale)}</span>
        </div>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.files")}</span>
          <span className={styles.tipMono}>{formatCount(node.files, t.locale)}</span>
        </div>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.lastOpened")}</span>
          <span className={styles.tipText}>{formatRelative(node.lastOpened, t.locale, now)}</span>
        </div>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.status")}</span>
          <span className={styles.tipStatus}>
            <StatusDot status={node.status} />
            {t(`status.${node.status}`)}
          </span>
        </div>
      </div>
    </div>
  );
}
