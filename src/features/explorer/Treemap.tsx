import { type CSSProperties, useCallback, useMemo, useState } from "react";
import { useStore } from "../../app/store";
import { useSize } from "../../app/useSize";
import { Menu, type MenuItem } from "../../components/Menu";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatCount, formatDelta, formatRelative } from "../../core/format";
import { squarify } from "../../core/treemap/squarify";
import { type Translator, useT } from "../../i18n";
import type { NodeId, TreeNode } from "../../ipc/types";
import pop from "../../components/Popover.module.css";
import { tileFill } from "./tileFill";
import styles from "./Treemap.module.css";

const GAP = 3;
const TOOLTIP_W = 268;
const PHANTOM = -1;

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

/** Opacité de la maquette : estompe tout sauf les tuiles concernées (et l'intérieur des parents estompés). */
function isDimmed(id: NodeId, parent: NodeId | null, keep: readonly NodeId[] | null): boolean {
  if (!keep) return false;
  if (keep.includes(id)) return false;
  if (parent !== null && !keep.includes(parent)) return false;
  return true;
}

export function Treemap({ root, highlight, onCopyPath, onUnavailable }: Props) {
  const t = useT();
  const mode = useStore((s) => s.colorMode);
  const zoom = useStore((s) => s.zoom);
  const setOpenReco = useStore((s) => s.setOpenReco);
  const checked = useStore((s) => s.checked);
  const toggleChecked = useStore((s) => s.toggleChecked);
  const [ref, size] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<{ id: NodeId; x: number; y: number } | null>(null);
  const [menu, setMenu] = useState<{ node: TreeNode; x: number; y: number } | null>(null);
  const [now] = useState(() => Date.now());

  const index = useMemo(() => {
    const map = new Map<NodeId, { node: TreeNode; parent: NodeId | null }>();
    for (const c of root.children) {
      map.set(c.id, { node: c, parent: null });
      for (const g of c.children) map.set(g.id, { node: g, parent: c.id });
    }
    return map;
  }, [root]);

  const tiles = useMemo(
    () => squarify(root.children, area, { x: 0, y: 0, w: size.width, h: size.height }, GAP),
    [root, size.width, size.height],
  );

  const activate = useCallback(
    (node: TreeNode) => {
      if (node.hasChildren) zoom(node.id);
      else if (node.reco) setOpenReco(node.reco);
    },
    [zoom, setOpenReco],
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
          label={t("menu.label", { name: menu.node.name })}
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

function Tile({ node, style, width, height, hoverId, highlight, now, mode, t, onActivate, onMenu }: TileProps) {
  const fill = tileFill(node, mode, now);
  const delta = deltaText(node, t);
  const small = width < 96 || height < 64;
  const size = formatBytes(node.size, t.locale);
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
      aria-label={t("explorer.tileLabel", { name: node.name, size })}
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
      {width >= 36 && height >= 22 && (
        <div className={styles.head}>
          <span className={styles.name}>{node.name}</span>
          {delta && (
            <span className={styles.delta} data-status={node.gainStatus}>
              {delta}
            </span>
          )}
        </div>
      )}
      {node.children.length > 0 && height >= 72 && width >= 48 && (
        <Children node={node} hoverId={hoverId} highlight={highlight} now={now} mode={mode} t={t} onActivate={onActivate} onMenu={onMenu} />
      )}
      {height >= 48 && width >= 36 && <span className={styles.foot}>{size}</span>}
    </div>
  );
}

type ChildrenProps = Omit<TileProps, "style" | "width" | "height">;

function Children({ node, hoverId, highlight, now, mode, t, onActivate, onMenu }: ChildrenProps) {
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
        const label = rect.w >= 34 && rect.h >= 18;
        return (
          <div
            key={item.id}
            data-node={item.id}
            className={styles.child}
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
            {label && (
              <>
                <span className={styles.childName}>{item.name}</span>
                {delta ? (
                  <span className={styles.delta} data-status={item.gainStatus}>
                    {delta}
                  </span>
                ) : (
                  <span className={styles.childSize}>{formatBytes(item.size, t.locale)}</span>
                )}
              </>
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
      <span className={styles.tipPath}>{node.path}</span>
      <div className={styles.tipGrid}>
        <div className={styles.tipCell}>
          <span className={styles.tipLabel}>{t("tooltip.size")}</span>
          <span className={styles.tipMono}>{formatBytes(node.size, t.locale)}</span>
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
