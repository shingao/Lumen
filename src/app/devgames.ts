// Jeux et Dev : résultats chargés à la demande et sélections, gardés quand on change d'écran.
// Rien n'est coché par défaut : ni un jeu (Lumen n'en supprime jamais), ni un cache de shaders,
// ni un artefact, ni un disque virtuel.

import { create } from "zustand";
import { backend } from "../ipc/client";
import type { DevOverview, DockerImages, GamesResult, ShaderCaches, WslDisks } from "../ipc/types";
import type { Load } from "./advanced";
import { useStore } from "./store";

export interface DevGamesState {
  games: Load<GamesResult>;
  covers: Record<string, string | null>;
  running: string[];
  /** Analyse à laquelle se rapportent `shaders` et `dev` ; tout est vidé si elle change. */
  scanId: number | null;
  shaders: Load<ShaderCaches>;
  shadersOn: boolean;
  dev: Load<DevOverview>;
  checkedArtifacts: Set<number>;
  checkedCaches: Set<string>;
  docker: Load<DockerImages>;
  wsl: Load<WslDisks>;
  checkedWsl: Set<number>;
  /** Message d'échec à l'ouverture d'un launcher. */
  openError: string | null;
}

export const useDevGames = create<DevGamesState>()(() => ({
  games: { status: "idle" },
  covers: {},
  running: [],
  scanId: null,
  shaders: { status: "idle" },
  shadersOn: false,
  dev: { status: "idle" },
  checkedArtifacts: new Set(),
  checkedCaches: new Set(),
  docker: { status: "idle" },
  wsl: { status: "idle" },
  checkedWsl: new Set(),
  openError: null,
}));

const set = (patch: Partial<DevGamesState>) => useDevGames.setState(patch);
const message = (err: unknown) => String(err);

/** Analyse affichée ; change → shaders et projets vidés (les nœuds ne s'y rapportent plus). */
function currentScan(): number | null {
  const scanId = useStore.getState().scan.summary?.scanId ?? null;
  if (useDevGames.getState().scanId !== scanId) {
    set({ scanId, shaders: { status: "idle" }, shadersOn: false, dev: { status: "idle" }, checkedArtifacts: new Set(), checkedCaches: new Set() });
  }
  return scanId;
}

export async function loadGames(): Promise<void> {
  set({ games: { status: "loading" } });
  try {
    const value = await backend.games();
    set({ games: { status: "done", value } });
    for (const g of value.games.filter((x) => x.cover)) void loadCover(g.id);
  } catch (err) {
    set({ games: { status: "failed", message: message(err) } });
  }
}

async function loadCover(id: string): Promise<void> {
  try {
    const url = await backend.gameCover(id);
    set({ covers: { ...useDevGames.getState().covers, [id]: url } });
  } catch {
    // Jaquette illisible : la vignette de repli reste.
  }
}

export async function refreshRunning(): Promise<void> {
  try {
    set({ running: await backend.gamesRunning() });
  } catch {
    set({ running: [] });
  }
}

export async function openGame(id: string, action: "uninstall" | "move"): Promise<void> {
  set({ openError: null });
  try {
    await backend.openGame(id, action);
  } catch (err) {
    set({ openError: message(err) });
  }
}

export async function loadShaders(): Promise<void> {
  const scanId = currentScan();
  set({ shaders: { status: "loading" } });
  try {
    set({ shaders: { status: "done", value: await backend.shaderCaches(scanId) } });
  } catch (err) {
    set({ shaders: { status: "failed", message: message(err) } });
  }
}

export function setShadersOn(on: boolean): void {
  set({ shadersOn: on });
}

/** Rechargé si l'analyse affichée a changé. */
export function ensureScanData(kind: "shaders" | "dev"): void {
  const before = useDevGames.getState().scanId;
  const scanId = currentScan();
  const s = useDevGames.getState();
  const stale = before !== scanId || s[kind].status === "idle";
  if (!stale) return;
  if (kind === "shaders") void loadShaders();
  else if (scanId !== null) void loadDev(scanId);
}

async function loadDev(scanId: number): Promise<void> {
  set({ dev: { status: "loading" } });
  try {
    const value = await backend.devOverview(scanId);
    if (useDevGames.getState().scanId === scanId) set({ dev: { status: "done", value } });
  } catch (err) {
    set({ dev: { status: "failed", message: message(err) } });
  }
}

function toggle<T>(current: Set<T>, ids: T[], on: boolean): Set<T> {
  const next = new Set(current);
  for (const id of ids) {
    if (on) next.add(id);
    else next.delete(id);
  }
  return next;
}

export function checkArtifacts(ids: number[], on: boolean): void {
  set({ checkedArtifacts: toggle(useDevGames.getState().checkedArtifacts, ids, on) });
}

export function checkCaches(ids: string[], on: boolean): void {
  set({ checkedCaches: toggle(useDevGames.getState().checkedCaches, ids, on) });
}

export function checkWsl(ids: number[], on: boolean): void {
  set({ checkedWsl: toggle(useDevGames.getState().checkedWsl, ids, on) });
}

export async function loadDocker(): Promise<void> {
  set({ docker: { status: "loading" } });
  try {
    set({ docker: { status: "done", value: await backend.dockerImages() } });
  } catch (err) {
    set({ docker: { status: "failed", message: message(err) } });
  }
}

export async function loadWsl(): Promise<void> {
  set({ wsl: { status: "loading" }, checkedWsl: new Set() });
  try {
    set({ wsl: { status: "done", value: await backend.wslDisks() } });
  } catch (err) {
    set({ wsl: { status: "failed", message: message(err) } });
  }
}

/** Après un nettoyage : projets, caches, Docker, WSL et shaders relus. */
export function resetDevGames(): void {
  set({ shaders: { status: "idle" }, shadersOn: false, dev: { status: "idle" }, checkedArtifacts: new Set(), checkedCaches: new Set(), docker: { status: "idle" }, wsl: { status: "idle" }, checkedWsl: new Set() });
}
