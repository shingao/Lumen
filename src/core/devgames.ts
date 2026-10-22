// Tris des écrans Jeux et Dev (logique pure).

import type { DevProject, Game } from "../ipc/types";

export type GameSort = "size" | "lastPlayed" | "name";
export type ProjectSort = "inactive" | "artifacts";

/** Par taille, par dernière partie (la plus ancienne d'abord, inconnue en dernier) ou par nom. */
export function sortGames(games: readonly Game[], sort: GameSort, locale: string): Game[] {
  const list = [...games];
  if (sort === "name") return list.sort((a, b) => a.name.localeCompare(b.name, locale));
  if (sort === "lastPlayed") return list.sort((a, b) => (a.lastPlayed ?? Number.MAX_SAFE_INTEGER) - (b.lastPlayed ?? Number.MAX_SAFE_INTEGER));
  return list.sort((a, b) => (b.size ?? 0) - (a.size ?? 0));
}

/** « Inactif depuis » : le plus ancien d'abord, sans date connue en dernier ; ou par taille d'artefacts. */
export function sortProjects(list: readonly DevProject[], sort: ProjectSort): DevProject[] {
  const copy = [...list];
  if (sort === "artifacts") return copy.sort((a, b) => b.artifactsSize - a.artifactsSize || a.name.localeCompare(b.name));
  return copy.sort((a, b) => (a.lastActivity ?? Number.MAX_SAFE_INTEGER) - (b.lastActivity ?? Number.MAX_SAFE_INTEGER));
}
