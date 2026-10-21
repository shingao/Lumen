// Logique pure de la courbe de la Timeline : plage de dates, échelles, graduations.

import { GIB } from "./format";

export type TimelineRange = "7d" | "30d" | "3m" | "all";

const DAY = 86_400_000;
const RANGE_DAYS: Record<TimelineRange, number | null> = { "7d": 7, "30d": 30, "3m": 91, all: null };

/** Date de début d'une plage (null : tout l'historique). */
export function rangeStart(range: TimelineRange, now: number): number | null {
  const days = RANGE_DAYS[range];
  return days === null ? null : now - days * DAY;
}

export function inRange<T extends { at: number }>(items: readonly T[], range: TimelineRange, now: number): T[] {
  const start = rangeStart(range, now);
  return start === null ? [...items] : items.filter((i) => i.at >= start);
}

const STEPS_GIB = [0.25, 0.5, 1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000];

/** Graduations « rondes » (en Gio) couvrant [min, max], environ `count` intervalles. */
export function byteTicks(min: number, max: number, count = 4): { lo: number; hi: number; ticks: number[] } {
  const span = Math.max(max - min, GIB / 4);
  const raw = span / count / GIB;
  const step = (STEPS_GIB.find((s) => s >= raw) ?? STEPS_GIB[STEPS_GIB.length - 1] ?? 1) * GIB;
  const lo = Math.floor(min / step) * step;
  let hi = Math.ceil(max / step) * step;
  if (hi === lo) hi = lo + step;
  const ticks: number[] = [];
  for (let v = lo; v <= hi + step / 2; v += step) ticks.push(v);
  return { lo, hi, ticks };
}

export interface Scale {
  x(at: number): number;
  y(bytes: number): number;
  ticks: number[];
  start: number;
  end: number;
}

/** Échelles d'une courbe dans un cadre ; le temps couvre la plage demandée, les octets ±5 %. */
export function scaleOf(points: readonly { at: number; used: number }[], box: { left: number; top: number; width: number; height: number }, start: number, end: number): Scale {
  const used = points.map((p) => p.used);
  const min = used.length > 0 ? Math.min(...used) : 0;
  const max = used.length > 0 ? Math.max(...used) : GIB;
  const pad = Math.max((max - min) * 0.08, GIB / 8);
  const { lo, hi, ticks } = byteTicks(Math.max(0, min - pad), max + pad);
  const span = Math.max(end - start, 1);
  return {
    x: (at) => box.left + ((at - start) / span) * box.width,
    y: (b) => box.top + box.height - ((b - lo) / (hi - lo)) * box.height,
    ticks,
    start,
    end,
  };
}

/** Le point le plus proche d'une date (sélection au clic). */
export function nearest<T extends { at: number }>(items: readonly T[], at: number): T | null {
  let best: T | null = null;
  for (const i of items) if (best === null || Math.abs(i.at - at) < Math.abs(best.at - at)) best = i;
  return best;
}
