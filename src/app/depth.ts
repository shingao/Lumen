// Analyse en profondeur (onglet Profondeur) : analyse du volume système entier, en LECTURE SEULE,
// classée en postes par le backend. Elle a son propre état : l'analyse affichée dans l'Explorer
// n'en est jamais remplacée (sauf si elle porte sur le même volume, qu'on rafraîchit alors).

import { create } from "zustand";
import { backend } from "../ipc/client";
import type { DepthReport, NodeId, RestorePoints, ScanMethod, ScanProgress, ScanTarget, TreeSlice } from "../ipc/types";
import { showCachedScan } from "./scanning";
import { useStore } from "./store";

export interface DepthState {
  status: "idle" | "running" | "done" | "failed";
  scanId: number | null;
  method: ScanMethod;
  progress: ScanProgress | null;
  report: DepthReport | null;
  /** Nœud affiché dans la treemap (null : racine du volume). */
  node: NodeId | null;
  slice: TreeSlice | null;
  message: string | null;
  /** Espace des clichés instantanés, lu en administrateur sur demande (une demande UAC). */
  restore: { status: "idle" | "loading" | "done" | "failed"; data: RestorePoints | null; message: string | null };
}

const INITIAL: DepthState = {
  status: "idle",
  scanId: null,
  method: "auto",
  progress: null,
  report: null,
  node: null,
  slice: null,
  message: null,
  restore: { status: "idle", data: null, message: null },
};

export const useDepth = create<DepthState>()(() => INITIAL);

/** Lecteur système : celui du dossier utilisateur (C: à défaut). */
export function systemDrive(): string {
  const profile = useStore.getState().appInfo?.profile ?? "";
  return /^[A-Za-z]:/.test(profile) ? profile.slice(0, 2).toUpperCase() : "C:";
}

const target = (): ScanTarget => ({ kind: "volume", letter: systemDrive() });

let subscribed = false;

function subscribe(): void {
  if (subscribed) return;
  subscribed = true;
  backend.onScanEvent((e) => {
    const s = useDepth.getState();
    if (s.status !== "running") return;
    const ours = (id: number) => s.scanId === id || s.scanId === null;
    if (e.type === "progress") {
      if (ours(e.progress.scanId)) useDepth.setState({ scanId: e.progress.scanId, progress: e.progress });
    } else if (e.type === "done") {
      if (!ours(e.summary.scanId)) return;
      void load(e.summary.scanId);
      // L'Explorer affichait ce même volume : sa nouvelle analyse remplace l'ancienne.
      const main = useStore.getState();
      if (main.target.kind === "volume" && main.target.letter.toUpperCase() === systemDrive() && main.scan.status !== "running") void showCachedScan(main.target);
    } else if (ours(e.scanId)) {
      useDepth.setState({ status: s.report ? "done" : "failed", progress: null, message: e.cancelled ? null : e.message });
    }
  });
}

async function load(scanId: number): Promise<void> {
  try {
    const [report, slice] = await Promise.all([backend.depthReport(scanId), backend.depthTree(scanId, null)]);
    useDepth.setState({ status: "done", scanId, report, slice, node: null, progress: null, message: null });
  } catch (err) {
    useDepth.setState({ status: "failed", progress: null, message: String(err) });
  }
}

/** Analyse du volume déjà gardée (cette session ou enregistrée) : affichée sans réanalyser. */
export async function loadCachedDepth(): Promise<void> {
  subscribe();
  if (useDepth.getState().status !== "idle") return;
  const summary = await backend.cachedScan(target()).catch(() => null);
  if (summary && useDepth.getState().status === "idle") await load(summary.scanId);
}

/** « Analyser en profondeur » : parcours sans droits (`auto`) ou lecture de la MFT par
 * l'assistant élevé (`fast`, si l'utilisateur l'accepte). */
export async function startDepth(method: ScanMethod): Promise<void> {
  subscribe();
  if (useDepth.getState().status === "running") return;
  useDepth.setState({ status: "running", scanId: null, method, progress: null, message: null });
  try {
    const id = await backend.startScan(target(), method);
    if (useDepth.getState().scanId === null) useDepth.setState({ scanId: id });
  } catch (err) {
    const s = useDepth.getState();
    useDepth.setState({ status: s.report ? "done" : "failed", message: String(err) });
  }
}

export async function cancelDepth(): Promise<void> {
  const { scanId, status } = useDepth.getState();
  if (status === "running" && scanId !== null) await backend.cancelScan(scanId);
}

/** Zoom dans la treemap du volume (null : racine). */
export async function openDepthNode(node: NodeId | null): Promise<void> {
  const { scanId } = useDepth.getState();
  if (scanId === null) return;
  try {
    const slice = await backend.depthTree(scanId, node);
    useDepth.setState({ slice, node: slice.root.id === 0 ? null : slice.root.id });
  } catch (err) {
    console.error("profondeur :", err);
  }
}

/** Espace des clichés instantanés (lecture seule, une demande UAC). */
export async function readShadowStorage(): Promise<void> {
  useDepth.setState({ restore: { status: "loading", data: null, message: null } });
  try {
    const data = await backend.readRestorePoints();
    useDepth.setState({ restore: { status: "done", data, message: null } });
  } catch (err) {
    useDepth.setState({ restore: { status: "failed", data: null, message: String(err) } });
  }
}

/** Remise à zéro (tests). */
export function resetDepth(): void {
  useDepth.setState(INITIAL, true);
}
