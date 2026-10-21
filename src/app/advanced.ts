// Nettoyage avancé : résultats des recherches (par analyse) et sélections, gardés quand on
// change d'onglet. Rien n'est coché par défaut : tout y est « À vérifier ».

import { create } from "zustand";
import { checkCopies, toggleMember } from "../core/duplicates";
import { backend } from "../ipc/client";
import type {
  DormantResult,
  DuplicatesProgress,
  DuplicatesResult,
  FileKind,
  LargestResult,
  RestorePoints,
  SystemAction,
  SystemSinks,
} from "../ipc/types";
import { useStore } from "./store";

export type Load<T> = { status: "idle" } | { status: "loading" } | { status: "done"; value: T } | { status: "failed"; message: string };

export interface AdvancedState {
  /** Analyse à laquelle se rapportent les résultats ; tout est vidé si elle change. */
  scanId: number | null;
  dormant: Load<DormantResult>;
  dormantMonths: number | null;
  largest: Load<LargestResult>;
  largestKind: FileKind | null;
  duplicates: Load<DuplicatesResult>;
  dupProgress: DuplicatesProgress | null;
  dupCancelling: boolean;
  sinks: Load<SystemSinks>;
  checkedDormant: Set<number>;
  checkedLargest: Set<number>;
  checkedDup: Set<number>;
  systemActions: SystemAction[];
  restorePoint: boolean;
  /** Points de restauration lus en administrateur : seule base de « supprimer le plus ancien ». */
  restorePoints: Load<RestorePoints>;
}

const EMPTY = (): Omit<AdvancedState, "sinks" | "systemActions" | "restorePoint" | "restorePoints"> => ({
  scanId: null,
  dormant: { status: "idle" },
  dormantMonths: null,
  largest: { status: "idle" },
  largestKind: null,
  duplicates: { status: "idle" },
  dupProgress: null,
  dupCancelling: false,
  checkedDormant: new Set(),
  checkedLargest: new Set(),
  checkedDup: new Set(),
});

export const useAdvanced = create<AdvancedState>()(() => ({
  ...EMPTY(),
  sinks: { status: "idle" },
  systemActions: [],
  restorePoint: true,
  restorePoints: { status: "idle" },
}));

const set = (patch: Partial<AdvancedState>) => useAdvanced.setState(patch);
const message = (err: unknown) => String(err);

/** Analyse affichée ; change → résultats vidés (les nœuds ne s'y rapportent plus). */
function currentScan(): number | null {
  const scanId = useStore.getState().scan.summary?.scanId ?? null;
  if (useAdvanced.getState().scanId !== scanId) set({ ...EMPTY(), scanId });
  return scanId;
}

export async function loadDormant(months: number): Promise<void> {
  const scanId = currentScan();
  if (scanId === null) return;
  set({ dormant: { status: "loading" }, dormantMonths: months, checkedDormant: new Set() });
  try {
    const value = await backend.dormant(scanId, months);
    if (useAdvanced.getState().dormantMonths === months) set({ dormant: { status: "done", value } });
  } catch (err) {
    set({ dormant: { status: "failed", message: message(err) } });
  }
}

export async function loadLargest(kind: FileKind | null): Promise<void> {
  const scanId = currentScan();
  if (scanId === null) return;
  set({ largest: { status: "loading" }, largestKind: kind });
  try {
    const value = await backend.largest(scanId, kind);
    if (useAdvanced.getState().largestKind === kind) set({ largest: { status: "done", value } });
  } catch (err) {
    set({ largest: { status: "failed", message: message(err) } });
  }
}

let dupSubscribed = false;

function subscribeDuplicates(): void {
  if (dupSubscribed) return;
  dupSubscribed = true;
  backend.onDuplicatesEvent((e) => {
    const s = useAdvanced.getState();
    if (e.type === "progress") {
      if (e.progress.scanId === s.scanId) set({ dupProgress: e.progress });
    } else if (e.type === "done") {
      if (e.result.scanId === s.scanId) set({ duplicates: { status: "done", value: e.result }, dupProgress: null, dupCancelling: false, checkedDup: new Set() });
    } else if (e.scanId === s.scanId) {
      set({ duplicates: e.cancelled ? { status: "idle" } : { status: "failed", message: e.message }, dupProgress: null, dupCancelling: false });
    }
  });
}

export async function startDuplicates(minSizeMb: number): Promise<void> {
  subscribeDuplicates();
  const scanId = currentScan();
  if (scanId === null) return;
  set({ duplicates: { status: "loading" }, dupProgress: null, dupCancelling: false, checkedDup: new Set() });
  try {
    await backend.startDuplicates(scanId, minSizeMb);
  } catch (err) {
    set({ duplicates: { status: "failed", message: message(err) } });
  }
}

export function cancelDuplicates(): void {
  const { scanId, duplicates } = useAdvanced.getState();
  if (scanId === null || duplicates.status !== "loading") return;
  set({ dupCancelling: true });
  void backend.cancelDuplicates(scanId);
}

export async function loadSinks(): Promise<void> {
  set({ sinks: { status: "loading" } });
  try {
    const value = await backend.systemSinks();
    // Protection du système désactivée : pas de point de restauration possible, on le dit.
    set({ sinks: { status: "done", value }, ...(value.restoreProtection === false ? { restorePoint: false } : {}) });
  } catch (err) {
    set({ sinks: { status: "failed", message: message(err) } });
  }
}

const toggled = (s: ReadonlySet<number>, ids: readonly number[], on: boolean) => {
  const next = new Set(s);
  for (const id of ids) {
    if (on) next.add(id);
    else next.delete(id);
  }
  return next;
};

export function checkDormant(ids: readonly number[], on: boolean): void {
  set({ checkedDormant: toggled(useAdvanced.getState().checkedDormant, ids, on) });
}

export function checkLargest(ids: readonly number[], on: boolean): void {
  set({ checkedLargest: toggled(useAdvanced.getState().checkedLargest, ids, on) });
}

/** Coche ou décoche un exemplaire, jamais le dernier de son groupe. */
export function toggleDuplicate(groupId: number, id: number): void {
  const { duplicates, checkedDup } = useAdvanced.getState();
  if (duplicates.status !== "done") return;
  const g = duplicates.value.groups.find((x) => x.id === groupId);
  if (g) set({ checkedDup: toggleMember(g, checkedDup, id) });
}

/** « Cocher les copies » : tous les groupes (ou un seul), l'exemplaire présélectionné gardé. */
export function checkAllCopies(groupId?: number): void {
  const { duplicates, checkedDup } = useAdvanced.getState();
  if (duplicates.status !== "done") return;
  let next: Set<number> = new Set(checkedDup);
  for (const g of duplicates.value.groups) if (groupId === undefined || g.id === groupId) next = checkCopies(g, next);
  set({ checkedDup: next });
}

export function clearDuplicates(): void {
  set({ checkedDup: new Set() });
}

export function toggleSystemAction(action: SystemAction, on: boolean): void {
  const exclusive: Partial<Record<SystemAction, SystemAction>> = { hibernateOff: "hibernateReduced", hibernateReduced: "hibernateOff" };
  const rest = useAdvanced.getState().systemActions.filter((a) => a !== action && a !== exclusive[action]);
  set({ systemActions: on ? [...rest, action] : rest });
}

/** Lecture des points de restauration (une demande UAC, lecture seule). */
export async function loadRestorePoints(): Promise<void> {
  set({ restorePoints: { status: "loading" } });
  try {
    const value = await backend.readRestorePoints();
    const actions = useAdvanced.getState().systemActions.filter((a) => a !== "oldestRestorePoint" || value.deletable);
    set({ restorePoints: { status: "done", value }, systemActions: actions });
  } catch (err) {
    set({ restorePoints: { status: "failed", message: message(err) }, systemActions: useAdvanced.getState().systemActions.filter((a) => a !== "oldestRestorePoint") });
  }
}

export function openSystemProtection(): void {
  void backend.openSystemProtection().catch((err: unknown) => useStore.getState().showToast(String(err)));
}

export function setRestorePoint(on: boolean): void {
  set({ restorePoint: on });
}
