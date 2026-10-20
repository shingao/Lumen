// Backend réel : commandes Rust typées (src-tauri/src/scanning.rs) et événements de l'analyse.
// Avec ipc/client.ts, seul fichier qui parle à Tauri.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { categoryOf } from "../core/categories";
import type { Locale } from "../core/format";
import type { Backend } from "./backend";
import { DEFAULT_SETTINGS } from "./mock";
import type {
  AppInfo,
  Category,
  Kpis,
  NodeKind,
  ScanEvent,
  ScanProgress,
  ScanSummary,
  Settings,
  TreeNode,
  TreeSlice,
  Volume,
  VolumeKind,
} from "./types";

interface TreeNodeDto {
  id: number;
  name: string;
  path: string;
  kind: NodeKind;
  size: number;
  logical: number;
  files: number;
  lastOpened: number;
  others: number | null;
  hasChildren: boolean;
  children: TreeNodeDto[];
}

interface VolumeDto {
  letter: string;
  label: string;
  fileSystem: string;
  kind: VolumeKind;
  total: number;
  free: number;
  mftCapable: boolean;
}

const SETTINGS_KEY = "lumen-settings";

function toNode(dto: TreeNodeDto, inherited: Category): TreeNode {
  const category = dto.others !== null ? inherited : categoryOf(dto.name, inherited);
  return {
    id: dto.id,
    name: dto.name,
    path: dto.path,
    kind: dto.kind,
    size: dto.size,
    logical: dto.logical,
    othersCount: dto.others,
    files: dto.files,
    lastOpened: dto.lastOpened,
    category,
    // Statuts, recommandations et gains : moteur de règles (jalon 4).
    status: "keep",
    reco: null,
    gain: 0,
    gainStatus: null,
    gainUpTo: false,
    hasChildren: dto.hasChildren,
    children: dto.children.map((c) => toNode(c, category)),
  };
}

function readSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<Settings>) };
  } catch {
    // Réglages par défaut.
  }
  return { ...DEFAULT_SETTINGS };
}

export function createTauriBackend(): Backend {
  let settings = readSettings();
  let volumes: Volume[] = [];

  const loadVolumes = async (): Promise<Volume[]> => {
    const list = await invoke<VolumeDto[]>("volumes_list");
    volumes = list.map((v) => ({
      id: v.letter,
      letter: v.letter,
      label: v.label,
      kind: v.kind,
      fileSystem: v.fileSystem,
      mftCapable: v.mftCapable,
      total: v.total,
      free: v.free,
    }));
    return volumes;
  };

  return {
    setLocale(_locale: Locale) {},

    appInfo: () => invoke<AppInfo>("app_info"),

    volumes: loadVolumes,

    startScan: (target, method) => invoke<number>("scan_start", { target, method }),

    cancelScan: (scanId) => invoke<void>("scan_cancel", { scanId }),

    onScanEvent(listener) {
      const offs = [
        listen<ScanProgress>("scan:progress", (e) => listener({ type: "progress", progress: e.payload })),
        listen<ScanSummary>("scan:done", (e) => listener({ type: "done", summary: e.payload })),
        listen<{ scanId: number; cancelled: boolean; message: string }>("scan:failed", (e) =>
          listener({ type: "failed", ...e.payload } satisfies ScanEvent),
        ),
      ];
      return () => offs.forEach((p) => void p.then((off) => off()));
    },

    async tree(scanId, node) {
      const slice = await invoke<{ root: TreeNodeDto; breadcrumb: { label: string; id: number | null }[] }>("scan_tree", {
        scanId,
        node,
      });
      const inherited = slice.breadcrumb.reduce<Category>((cat, c) => categoryOf(c.label, cat), "tools");
      const result: TreeSlice = { root: toNode(slice.root, inherited), breadcrumb: slice.breadcrumb };
      return result;
    },

    async kpis(target, summary) {
      const list = volumes.length > 0 ? volumes : await loadVolumes();
      const letter = target.kind === "volume" ? target.letter : (summary?.volume ?? "C:");
      const vol = list.find((v) => v.letter.toUpperCase() === letter.toUpperCase());
      const free = vol?.free ?? 0;
      const used = vol ? vol.total - vol.free : 0;
      const k: Kpis = {
        rulesReady: false,
        freeNow: free,
        freeAfter: free,
        safeBytes: 0,
        safeCount: 0,
        reviewBytes: 0,
        reviewCount: 0,
        // Ce que l'analyse n'a pas couvert sur ce disque : système, applis, autres dossiers.
        unanalyzed: summary ? Math.max(0, used - summary.allocated) : 0,
        trend7d: 0,
      };
      return k;
    },

    recommendations: () => Promise.resolve([]),

    recommendation: (id) => Promise.reject(new Error(`recommandation inconnue : ${id}`)),

    script: () => Promise.resolve(""),

    // Recherche dans l'index du scan : jalon 9.
    search: () => Promise.resolve([]),

    settings: () => Promise.resolve({ ...settings }),

    updateSettings(patch) {
      settings = { ...settings, ...patch };
      try {
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
      } catch {
        // Non persistant : sans conséquence.
      }
      return Promise.resolve({ ...settings });
    },
  };
}
