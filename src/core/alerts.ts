// Alertes d'espace libre (logique pure) : un disque sous le seuil reçoit au plus une notification
// par jour. Les disques sont retenus par leur clé stable (GUID), jamais par leur lettre.

import type { Volume } from "../ipc/types";

/** Jour local `AAAA-MM-JJ`. */
export function localDay(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function freeRatio(v: Pick<Volume, "free" | "total">): number {
  return v.total > 0 ? v.free / v.total : 1;
}

/** Disques locaux sous le seuil (%) qui n'ont pas encore été signalés aujourd'hui. */
export function dueAlerts(volumes: readonly Volume[], thresholdPct: number, sent: Readonly<Record<string, string>>, today: string): Volume[] {
  return volumes.filter((v) => (v.kind === "fixed" || v.kind === "removable") && v.total > 0 && freeRatio(v) < thresholdPct / 100 && sent[v.key] !== today);
}

/** Alertes du jour ajoutées ; les jours passés sont oubliés (rien ne grossit avec le temps). */
export function markSent(sent: Readonly<Record<string, string>>, keys: readonly string[], today: string): Record<string, string> {
  const next: Record<string, string> = {};
  for (const [k, day] of Object.entries(sent)) if (day === today) next[k] = day;
  for (const k of keys) next[k] = today;
  return next;
}
