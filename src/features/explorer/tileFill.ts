// Remplissage d'une tuile selon le mode de coloration. Uniquement des variables CSS.

import type { ColorMode } from "../../app/store";
import type { Category, TreeNode } from "../../ipc/types";

const DAY = 86_400_000;

/** Palier d'âge (0 = cette semaine … 4 = plus d'un an) d'après la dernière ouverture. */
export function ageBucket(lastOpened: number, now: number): 0 | 1 | 2 | 3 | 4 {
  const days = (now - lastOpened) / DAY;
  if (days < 7) return 0;
  if (days < 31) return 1;
  if (days < 92) return 2;
  if (days < 366) return 3;
  return 4;
}

const CATEGORY_VAR: Record<Category, string> = {
  media: "var(--cat-media)",
  code: "var(--cat-code)",
  packages: "var(--cat-packages)",
  tools: "var(--cat-tools)",
  android: "var(--cat-android)",
  ai: "var(--cat-ai)",
  games: "var(--cat-games)",
  system: "var(--st-keep)",
};

export const CATEGORY_ORDER: readonly Category[] = ["media", "code", "packages", "tools", "android", "ai", "games"];

export function categoryFill(category: Category): string {
  return CATEGORY_VAR[category];
}

export interface Fill {
  bg: string;
  hatch: boolean;
  /** Fond foncé du mode Âge : le texte secondaire passe au ton fort pour garder le contraste. */
  deep: boolean;
}

export function tileFill(node: TreeNode, mode: ColorMode, now: number): Fill {
  // Regroupement « autres » : neutre dans tous les modes.
  if (node.othersCount !== null) return { bg: "var(--st-keep)", hatch: false, deep: false };
  const hatch = node.status === "safe";
  if (mode === "category") return { bg: CATEGORY_VAR[node.category], hatch, deep: false };
  if (mode === "cleanup") return { bg: `var(--st-${node.status})`, hatch, deep: false };
  const bucket = ageBucket(node.lastOpened, now);
  return { bg: `var(--age-${bucket})`, hatch, deep: bucket >= 3 };
}
