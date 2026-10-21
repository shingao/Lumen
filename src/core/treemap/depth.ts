// Profondeur adaptative de la treemap : une sous-tuile assez grande montre ses propres enfants,
// jusqu'à 4 niveaux sous le dossier affiché. Plus de grande tuile vide.

export const MAX_LEVEL = 4;
export const GROUP_MIN_W = 120;
export const GROUP_MIN_H = 80;
/** En dessous, la taille d'une sous-tuile ne tient pas à côté de son nom : elle n'est pas affichée. */
export const SIZE_MIN_W = 76;

/** Une sous-tuile de niveau `level` (2 à 4) de `w` × `h` px montre-t-elle ses enfants ? */
export function nests(childCount: number, level: number, w: number, h: number): boolean {
  return childCount > 0 && level < MAX_LEVEL && w >= GROUP_MIN_W && h >= GROUP_MIN_H;
}
