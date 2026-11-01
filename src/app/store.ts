// État de l'interface (zustand). Les données viennent du backend ; ici, seulement ce que l'écran affiche.

import { create } from "zustand";
import type { Locale } from "../core/format";
import type {
  AppInfo,
  CleanMode,
  CleanPlan,
  CleanProgress,
  CleanReport,
  CleanRequest,
  NodeId,
  ScanMethod,
  ScanProgress,
  ScanSummary,
  ScanTarget,
  Settings,
} from "../ipc/types";

export type Screen = "analyse" | "timeline" | "cleanup" | "dev" | "games" | "journal" | "depth";
export const SCREENS: readonly Screen[] = ["analyse", "timeline", "cleanup", "dev", "games", "journal", "depth"];

/** Onglet du Nettoyage avancé (l'écran Profondeur ouvre directement « Gouffres système »). */
export type CleanupTab = "dormant" | "duplicates" | "large" | "system";

export type ColorMode = "category" | "cleanup" | "age";

export interface Toast {
  id: number;
  text: string;
}

export interface ScanState {
  id: number | null;
  status: "idle" | "running" | "done" | "failed";
  method: ScanMethod;
  progress: ScanProgress | null;
  /** Dernière analyse terminée de la cible affichée. */
  summary: ScanSummary | null;
  /** Message d'échec (en français, venu du backend). */
  message: string | null;
}

/** Nettoyage en cours d'affichage : plan à confirmer, exécution, bilan. */
export interface CleanUi {
  /** loading : plan en préparation ; confirm ; permanent : seconde confirmation (saisie) ; running ; report ; error. */
  step: "closed" | "loading" | "confirm" | "permanent" | "running" | "report" | "error";
  request: CleanRequest | null;
  mode: CleanMode;
  plan: CleanPlan | null;
  progress: CleanProgress | null;
  report: CleanReport | null;
  message: string | null;
  cancelling: boolean;
  /** « Continuer sans point de restauration » coché (actions système sans point préalable). */
  waiveRestorePoint: boolean;
}

export const CLEAN_CLOSED: CleanUi = {
  step: "closed",
  request: null,
  mode: "trash",
  plan: null,
  progress: null,
  report: null,
  message: null,
  cancelling: false,
  waiveRestorePoint: false,
};

export interface State {
  screen: Screen;
  settingsOpen: boolean;
  paletteOpen: boolean;
  settings: Settings | null;
  /** Langue effective (réglage ou langue du système). */
  locale: Locale;
  /** Thème effectif. */
  dark: boolean;

  appInfo: AppInfo | null;
  /** Ce qu'on analyse : le dossier utilisateur (par défaut) ou un disque entier. */
  target: ScanTarget;
  scan: ScanState;
  /** Nœud zoomé dans l'Explorer (null = racine analysée). */
  node: NodeId | null;
  colorMode: ColorMode;
  checked: string[];
  openReco: string | null;
  hoverReco: string | null;
  /** « ? » d'un KPI ou d'un en-tête de groupe : tuiles de ce statut mises en évidence dans la
   * treemap (le reste estompé) et bulle d'explication à côté du bouton cliqué. Garde son état
   * pendant le zoom et la navigation ← →. */
  statusFocus: StatusFocus | null;
  toast: Toast | null;
  clean: CleanUi;
  /** Incrémenté à chaque entrée ajoutée au journal ou restauration (rechargement de l'écran). */
  journalVersion: number;
  cleanupTab: CleanupTab;

  go(screen: Screen): void;
  setSettingsOpen(open: boolean): void;
  setPaletteOpen(open: boolean): void;
  setTarget(target: ScanTarget): void;
  setScan(patch: Partial<ScanState>): void;
  zoom(node: NodeId | null): void;
  setColorMode(mode: ColorMode): void;
  toggleChecked(id: string): void;
  setChecked(ids: string[]): void;
  setOpenReco(id: string | null): void;
  setHoverReco(id: string | null): void;
  setStatusFocus(focus: StatusFocus | null): void;
  showToast(text: string): void;
  dismissToast(): void;
  setClean(patch: Partial<CleanUi>): void;
  bumpJournal(): void;
  setCleanupTab(tab: CleanupTab): void;
}

/** Statut mis en évidence et bouton qui l'a demandé (la bulle s'affiche à côté de lui). */
export interface StatusFocus {
  status: "safe" | "review";
  from: "kpi" | "reco";
}

let toastId = 0;

export const useStore = create<State>()((set) => ({
  screen: "analyse",
  settingsOpen: false,
  paletteOpen: false,
  settings: null,
  locale: "fr",
  dark: false,

  appInfo: null,
  target: { kind: "userProfile" },
  scan: { id: null, status: "idle", method: "auto", progress: null, summary: null, message: null },
  node: null,
  colorMode: "category",
  checked: [],
  openReco: null,
  hoverReco: null,
  statusFocus: null,
  toast: null,
  clean: CLEAN_CLOSED,
  journalVersion: 0,
  cleanupTab: "dormant",

  // Changer d'onglet ferme le panneau de détail (et sa mise en évidence).
  go: (screen) => set({ screen, settingsOpen: false, openReco: null, hoverReco: null }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setTarget: (target) => set({ target, node: null, openReco: null, hoverReco: null }),
  setScan: (patch) => set((s) => ({ scan: { ...s.scan, ...patch } })),
  // Zoomer ou naviguer (fil d'Ariane, ← →, Étendre) ferme le panneau de détail : sa mise en
  // évidence ne suit jamais l'utilisateur ailleurs. Le « ? » d'un statut, lui, reste actif.
  zoom: (node) => set({ node, openReco: null, hoverReco: null }),
  setColorMode: (colorMode) => set({ colorMode }),
  toggleChecked: (id) =>
    set((s) => ({ checked: s.checked.includes(id) ? s.checked.filter((c) => c !== id) : [...s.checked, id] })),
  setChecked: (checked) => set({ checked }),
  // Une seule mise en évidence persistante : ouvrir le panneau arrête le « ? » d'un statut.
  setOpenReco: (openReco) => set((s) => ({ openReco, hoverReco: null, statusFocus: openReco ? null : s.statusFocus })),
  setHoverReco: (hoverReco) => set({ hoverReco }),
  // … et activer le « ? » d'un statut ferme le panneau.
  setStatusFocus: (statusFocus) => set((s) => ({ statusFocus, openReco: statusFocus ? null : s.openReco })),
  showToast: (text) => set({ toast: { id: ++toastId, text } }),
  dismissToast: () => set({ toast: null }),
  setClean: (patch) => set((s) => ({ clean: { ...s.clean, ...patch } })),
  bumpJournal: () => set((s) => ({ journalVersion: s.journalVersion + 1 })),
  setCleanupTab: (cleanupTab) => set({ cleanupTab }),
}));
