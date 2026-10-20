// État de l'interface (zustand). Les données viennent du backend ; ici, seulement ce que l'écran affiche.

import { create } from "zustand";
import type { Locale } from "../core/format";
import type { NodeId, Settings } from "../ipc/types";

export type Screen = "analyse" | "timeline" | "cleanup" | "dev" | "games" | "journal";
export const SCREENS: readonly Screen[] = ["analyse", "timeline", "cleanup", "dev", "games", "journal"];

export type ColorMode = "category" | "cleanup" | "age";

export interface Toast {
  id: number;
  text: string;
}

export interface State {
  screen: Screen;
  settingsOpen: boolean;
  paletteOpen: boolean;
  settings: Settings | null;
  /** Langue effective (réglage ou langue du système). */
  locale: Locale;
  /** Thème effectif. */
  dark: boolean;

  volume: string;
  /** Nœud zoomé dans l'Explorer (null = racine analysée). */
  node: NodeId | null;
  colorMode: ColorMode;
  checked: string[];
  openReco: string | null;
  hoverReco: string | null;
  toast: Toast | null;

  go(screen: Screen): void;
  setSettingsOpen(open: boolean): void;
  setPaletteOpen(open: boolean): void;
  setVolume(volume: string): void;
  zoom(node: NodeId | null): void;
  setColorMode(mode: ColorMode): void;
  toggleChecked(id: string): void;
  setChecked(ids: string[]): void;
  setOpenReco(id: string | null): void;
  setHoverReco(id: string | null): void;
  showToast(text: string): void;
  dismissToast(): void;
}

let toastId = 0;

export const useStore = create<State>()((set) => ({
  screen: "analyse",
  settingsOpen: false,
  paletteOpen: false,
  settings: null,
  locale: "fr",
  dark: false,

  volume: "C:",
  node: null,
  colorMode: "category",
  checked: [],
  openReco: null,
  hoverReco: null,
  toast: null,

  go: (screen) => set({ screen, settingsOpen: false }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setVolume: (volume) => set({ volume, node: null, openReco: null, hoverReco: null }),
  zoom: (node) => set({ node }),
  setColorMode: (colorMode) => set({ colorMode }),
  toggleChecked: (id) =>
    set((s) => ({ checked: s.checked.includes(id) ? s.checked.filter((c) => c !== id) : [...s.checked, id] })),
  setChecked: (checked) => set({ checked }),
  setOpenReco: (openReco) => set({ openReco, hoverReco: null }),
  setHoverReco: (hoverReco) => set({ hoverReco }),
  showToast: (text) => set({ toast: { id: ++toastId, text } }),
  dismissToast: () => set({ toast: null }),
}));
