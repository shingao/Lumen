// Timeline : séries de snapshots, série affichée, plage, comparaison de deux dates et état du
// snapshot quotidien. Le snapshot quotidien n'existe que tant que l'appli tourne : l'interface
// le demande au backend à intervalles réguliers (aucun service, aucune tâche planifiée).

import { create } from "zustand";
import type { TimelineRange } from "../core/timeline";
import { backend } from "../ipc/client";
import type { DailyStatus, SeriesKey, TimelineCompare, TimelineData, TimelineSeries } from "../ipc/types";
import type { Load } from "./advanced";
import { useStore } from "./store";

export interface TimelineState {
  series: Load<TimelineSeries[]>;
  selected: SeriesKey | null;
  range: TimelineRange;
  data: Load<TimelineData>;
  /** Snapshots comparés (identifiants) ; null : premier et dernier de la plage. */
  from: number | null;
  to: number | null;
  compare: Load<TimelineCompare>;
  daily: DailyStatus | null;
}

export const useTimeline = create<TimelineState>()(() => ({
  series: { status: "idle" },
  selected: null,
  range: "30d",
  data: { status: "idle" },
  from: null,
  to: null,
  compare: { status: "idle" },
  daily: null,
}));

const set = (patch: Partial<TimelineState>) => useTimeline.setState(patch);

export const sameSeries = (a: SeriesKey | null, b: SeriesKey | null) =>
  a !== null && b !== null && a.volumeId === b.volumeId && a.root.toLowerCase() === b.root.toLowerCase() && a.method === b.method;

/** Série par défaut : celle de la cible affichée dans l'Explorer, sinon la plus récente. */
function defaultSeries(list: TimelineSeries[]): TimelineSeries | null {
  const target = useStore.getState().target;
  const match = list.find((s) => s.target !== null && JSON.stringify(s.target) === JSON.stringify(target));
  return match ?? list[0] ?? null;
}

export async function loadTimeline(): Promise<void> {
  if (useTimeline.getState().series.status === "idle") set({ series: { status: "loading" } });
  try {
    const list = await backend.timelineSeries();
    const { selected } = useTimeline.getState();
    const keep = list.find((s) => sameSeries(s, selected)) ?? defaultSeries(list);
    set({ series: { status: "done", value: list } });
    if (keep) await selectSeries({ volumeId: keep.volumeId, root: keep.root, method: keep.method }, false);
    else set({ selected: null, data: { status: "idle" }, compare: { status: "idle" } });
  } catch (e) {
    set({ series: { status: "failed", message: String(e) } });
  }
}

export async function selectSeries(key: SeriesKey, reset = true): Promise<void> {
  const changed = !sameSeries(key, useTimeline.getState().selected);
  set({ selected: key, ...(changed || reset ? { from: null, to: null, compare: { status: "idle" } } : {}) });
  if (useTimeline.getState().data.status !== "done" || changed) set({ data: { status: "loading" } });
  try {
    const value = await backend.timeline(key);
    if (sameSeries(useTimeline.getState().selected, key)) set({ data: { status: "done", value } });
  } catch (e) {
    set({ data: { status: "failed", message: String(e) } });
  }
}

export function setRange(range: TimelineRange): void {
  set({ range, from: null, to: null, compare: { status: "idle" } });
}

export function setCompare(from: number | null, to: number | null): void {
  set({ from, to, compare: { status: "idle" } });
}

export async function runCompare(from: number, to: number): Promise<void> {
  set({ from, to, compare: { status: "loading" } });
  try {
    const value = await backend.timelineCompare(from, to);
    const s = useTimeline.getState();
    if (s.from === from && s.to === to) set({ compare: { status: "done", value } });
  } catch (e) {
    set({ compare: { status: "failed", message: String(e) } });
  }
}

/** Demande au backend un snapshot quotidien s'il est dû (au plus un à la fois). */
export async function dailyTick(): Promise<void> {
  const enabled = useStore.getState().settings?.dailySnapshot ?? true;
  try {
    set({ daily: await backend.timelineDaily(enabled) });
  } catch {
    // Pas de base, pas de volume : rien à faire, l'interface n'en dépend pas.
  }
}

/** Pour les tests : état initial. */
export function resetTimeline(): void {
  set({ series: { status: "idle" }, selected: null, range: "30d", data: { status: "idle" }, from: null, to: null, compare: { status: "idle" }, daily: null });
}
