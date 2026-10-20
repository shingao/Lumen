// Backend factice : sert les données de la maquette avec la même forme que le futur backend Rust.

import type { Backend } from "../backend";
import type { Locale } from "../../core/format";
import { GIB } from "../../core/format";
import { fuzzyFilter } from "../../core/fuzzy";
import type {
  Crumb,
  Kpis,
  NodeId,
  Recommendation,
  RecommendationDetail,
  SearchHit,
  Settings,
  TreeNode,
  TreeSlice,
  Volume,
} from "../types";
import type { L10n, MockReco, MockTile, When } from "./data";
import { D_TILES, KPI, RECOS, TILES, USER_ROOT, VOLUMES } from "./data";

const SETTINGS_KEY = "lumen-dev-settings";

export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  language: "system",
  excludedFolders: [],
  dormantMonths: 6,
  duplicateMinMb: 1,
  permanentDelete: false,
  dailySnapshot: true,
  trayIcon: false,
  lowSpaceAlert: true,
  lowSpaceThreshold: 10,
  explorerMenu: true,
  logsFolder: "%LOCALAPPDATA%\\Lumen\\logs",
};

const gb = (n: number) => Math.round(n * GIB);

function resolveWhen(when: When, now: number): number {
  if ("msAgo" in when) return now - when.msAgo;
  const d = new Date(now);
  d.setDate(d.getDate() - when.daysAgo);
  if (when.at) d.setHours(when.at[0], when.at[1], 0, 0);
  return d.getTime();
}

interface VolumeTree {
  rootId: NodeId;
  /** Segments au-dessus de la racine analysée (non cliquables) puis la racine. */
  prefix: string[];
  base: string;
  nodes: Map<NodeId, { tile: MockTile | null; parent: NodeId | null; children: NodeId[] }>;
  byKey: Map<string, NodeId>;
}

function buildVolume(tiles: MockTile[], rootId: NodeId, prefix: string[], base: string): VolumeTree {
  const nodes: VolumeTree["nodes"] = new Map();
  const byKey = new Map<string, NodeId>();
  nodes.set(rootId, { tile: null, parent: null, children: [] });
  tiles.forEach((tile, i) => byKey.set(tile.id, rootId + 1 + i));
  tiles.forEach((tile) => {
    const id = byKey.get(tile.id) as NodeId;
    const parent = tile.parent ? (byKey.get(tile.parent) as NodeId) : rootId;
    nodes.set(id, { tile, parent, children: [] });
  });
  tiles.forEach((tile) => {
    const id = byKey.get(tile.id) as NodeId;
    const parent = nodes.get(id)?.parent;
    if (parent !== undefined && parent !== null) nodes.get(parent)?.children.push(id);
  });
  return { rootId, prefix, base, nodes, byKey };
}

const TREES: Record<string, VolumeTree> = {
  "C:": buildVolume(TILES, 0, [USER_ROOT.volume, ...USER_ROOT.segments], "C:\\Users\\Arthur\\"),
  "D:": buildVolume(D_TILES, 10_000, ["D:"], "D:\\"),
};

function readSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<Settings>) };
  } catch {
    // Stockage indisponible : réglages par défaut.
  }
  return { ...DEFAULT_SETTINGS };
}

function writeSettings(s: Settings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Non persistant dans ce contexte : sans conséquence pour la démo.
  }
}

export function createMockBackend(now: () => number = Date.now): Backend {
  let locale: Locale = "fr";
  let settings = readSettings();
  const l = (v: L10n | string) => (typeof v === "string" ? v : v[locale]);

  const treeOf = (volume: string): VolumeTree => TREES[volume] ?? (TREES["C:"] as VolumeTree);

  function sizeOf(tree: VolumeTree, id: NodeId): number {
    const n = tree.nodes.get(id);
    if (!n) return 0;
    if (n.tile) return n.tile.size;
    return n.children.reduce((sum, c) => sum + sizeOf(tree, c), 0);
  }

  function toNode(tree: VolumeTree, id: NodeId, depth: number): TreeNode {
    const n = tree.nodes.get(id);
    const t = n?.tile ?? null;
    const children = depth > 0 ? (n?.children ?? []).map((c) => toNode(tree, c, depth - 1)) : [];
    if (!t) {
      const name = tree.prefix[tree.prefix.length - 1] ?? "";
      return {
        id,
        name,
        path: tree.base.replace(/\\$/, ""),
        size: gb(sizeOf(tree, id)),
        files: children.reduce((s, c) => s + c.files, 0),
        lastOpened: Math.max(0, ...children.map((c) => c.lastOpened)),
        category: "system",
        status: "keep",
        reco: null,
        gain: 0,
        gainStatus: null,
        gainUpTo: false,
        hasChildren: (n?.children.length ?? 0) > 0,
        children,
      };
    }
    return {
      id,
      name: l(t.name),
      path: tree.base + l(t.path),
      size: gb(t.size),
      ...(t.layoutSize !== undefined ? { layoutSize: gb(t.layoutSize) } : {}),
      files: t.files,
      lastOpened: resolveWhen(t.last, now()),
      category: t.category,
      status: t.status,
      reco: t.reco,
      gain: gb(t.gain ?? 0),
      gainStatus: t.gainStatus ?? null,
      gainUpTo: t.gainUpTo ?? false,
      hasChildren: (n?.children.length ?? 0) > 0,
      children,
    };
  }

  function breadcrumb(tree: VolumeTree, id: NodeId): Crumb[] {
    const chain: Crumb[] = [];
    let cur: NodeId | null = id;
    while (cur !== null && cur !== tree.rootId) {
      const n = tree.nodes.get(cur);
      if (!n?.tile) break;
      chain.unshift({ label: l(n.tile.name), id: cur });
      cur = n.parent;
    }
    const above = tree.prefix.slice(0, -1).map((label) => ({ label, id: null }));
    return [...above, { label: tree.prefix[tree.prefix.length - 1] ?? "", id: tree.rootId }, ...chain];
  }

  const recosOf = (volume: string): MockReco[] => (volume === "C:" ? RECOS : []);

  function toReco(r: MockReco): Recommendation {
    const tree = TREES["C:"] as VolumeTree;
    return {
      id: r.id,
      status: r.status,
      name: l(r.name),
      path: l(r.path),
      size: gb(r.size),
      highlight: r.highlight.map((k) => tree.byKey.get(k)).filter((v): v is NodeId => v !== undefined),
    };
  }

  function scriptLines(r: MockReco, whatIf: boolean): string[] {
    return r.script.map((line) => l(line).replace("§", whatIf ? " -WhatIf" : ""));
  }

  const notes = {
    safe: {
      fr: "Suppression définitive : ces fichiers ne passent pas par la Corbeille.",
      en: "Permanent deletion: these files do not go to the Recycle Bin.",
    },
    review: {
      fr: "Rien ne part sans ta confirmation. Relis la liste avant de lancer le script.",
      en: "Nothing leaves without your confirmation. Read the list before running the script.",
    },
  };

  const resolved = <T>(value: T): Promise<T> => Promise.resolve(value);

  return {
    setLocale(next) {
      locale = next;
    },

    volumes() {
      return resolved<Volume[]>(
        VOLUMES.map((v) => ({ id: v.id, letter: v.letter, label: l(v.label), kind: v.kind, total: gb(v.total), free: gb(v.free) })),
      );
    },

    kpis(volume) {
      const recos = recosOf(volume);
      const sum = (status: string) => recos.filter((r) => r.status === status).reduce((s, r) => s + r.size, 0);
      const count = (status: string) => recos.filter((r) => r.status === status).length;
      const vol = VOLUMES.find((v) => v.id === volume);
      const freeNow = volume === "C:" ? KPI.freeNow : (vol?.free ?? 0);
      const k: Kpis = {
        freeNow: gb(freeNow),
        freeAfter: gb(freeNow + sum("safe")),
        safeBytes: gb(sum("safe")),
        safeCount: count("safe"),
        reviewBytes: gb(sum("review")),
        reviewCount: count("review"),
        unanalyzed: gb(volume === "C:" ? KPI.unanalyzed : 0),
        trend7d: gb(volume === "C:" ? KPI.trend7d : 2.4),
      };
      return resolved(k);
    },

    tree(volume, node) {
      const tree = treeOf(volume);
      const id = node !== null && tree.nodes.has(node) ? node : tree.rootId;
      const slice: TreeSlice = { root: toNode(tree, id, 2), breadcrumb: breadcrumb(tree, id) };
      return resolved(slice);
    },

    recommendations(volume) {
      return resolved(recosOf(volume).map(toReco));
    },

    recommendation(id) {
      const r = RECOS.find((x) => x.id === id);
      if (!r) return Promise.reject(new Error(`recommandation inconnue : ${id}`));
      const detail: RecommendationDetail = {
        ...toReco(r),
        explanation: l(r.explanation),
        meta: l(r.meta),
        files: r.files.map(([path, size]) => ({ path: l(path), size: gb(size) })),
        script: r.script.map((line) => {
          const text = l(line);
          const at = text.indexOf("§");
          return { text: text.replace("§", ""), whatIfAt: at < 0 ? null : at };
        }),
        note: l(notes[r.status]),
      };
      return resolved(detail);
    },

    script(ids, whatIf) {
      const parts = RECOS.filter((r) => ids.includes(r.id)).map((r) => scriptLines(r, whatIf).join("\n"));
      return resolved(parts.join("\n\n"));
    },

    search(query, limit) {
      const hits: SearchHit[] = [];
      for (const tree of Object.values(TREES)) {
        for (const [id, n] of tree.nodes) {
          if (!n.tile) continue;
          hits.push({
            id,
            name: l(n.tile.name),
            path: tree.base + l(n.tile.path),
            size: gb(n.tile.size),
            modified: resolveWhen(n.tile.last, now()),
            kind: n.tile.files > 1 ? "dir" : "file",
          });
        }
      }
      return resolved(fuzzyFilter(hits, query, (h) => `${h.name} ${h.path}`, limit));
    },

    settings() {
      return resolved({ ...settings });
    },

    updateSettings(patch) {
      settings = { ...settings, ...patch };
      writeSettings(settings);
      return resolved({ ...settings });
    },
  };
}
