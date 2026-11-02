// Recommandations cochées d'office : les « Sans risque » seulement, sauf celles que leur règle
// déclare « precheck = false » (caches d'applications : l'appli doit être fermée). Un « À
// vérifier » n'est jamais coché d'office, ni par « Cocher tout le sans risque ».

import type { Recommendation } from "../ipc/types";

export function prechecked(r: Pick<Recommendation, "status" | "precheck">): boolean {
  return r.status === "safe" && r.precheck !== false;
}

/** Identifiants cochés d'office parmi `recos`. */
export function defaultChecked(recos: readonly Pick<Recommendation, "id" | "status" | "precheck">[]): string[] {
  return recos.filter(prechecked).map((r) => r.id);
}
