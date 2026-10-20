// Recherche floue légère (sous-séquence, insensible à la casse et aux accents).
// Le vrai index du scan vivra en Rust ; ceci sert aux commandes de la palette et au backend factice.

export function fold(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/**
 * Score d'une cible pour une requête déjà pliée : null si les lettres n'apparaissent pas dans l'ordre.
 * Bonus pour les lettres consécutives et les débuts de mot, malus pour la longueur.
 */
export function fuzzyScore(query: string, target: string): number | null {
  if (query.length === 0) return 0;
  const t = fold(target);
  let qi = 0;
  let score = 0;
  let prev = -2;
  for (let i = 0; i < t.length && qi < query.length; i++) {
    if (t[i] !== query[qi]) continue;
    const boundary = i === 0 || /[\s\\/._\-·]/.test(t[i - 1] ?? "");
    score += 1 + (i === prev + 1 ? 3 : 0) + (boundary ? 2 : 0);
    prev = i;
    qi++;
  }
  if (qi < query.length) return null;
  return score - t.length * 0.01;
}

export function fuzzyFilter<T>(items: readonly T[], query: string, key: (item: T) => string, limit: number): T[] {
  const q = fold(query.trim());
  const scored: { item: T; score: number }[] = [];
  for (const item of items) {
    const score = fuzzyScore(q, key(item));
    if (score !== null) scored.push({ item, score });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, limit).map((s) => s.item);
}
