// Sélection des doublons : on coche les exemplaires à supprimer, jamais tous ceux d'un groupe
// (safety le refuse aussi). Rien n'est coché par défaut ; « Cocher les copies » garde
// l'exemplaire présélectionné (le plus ancien).

export interface DupGroupLike {
  members: readonly { id: number; size: number }[];
  keep: number;
}

/** Peut-on cocher `id` ? Non s'il ne resterait plus aucun exemplaire du groupe. */
export function canCheck(group: DupGroupLike, checked: ReadonlySet<number>, id: number): boolean {
  if (checked.has(id)) return true;
  const left = group.members.filter((m) => !checked.has(m.id) && m.id !== id).length;
  return left >= 1;
}

/** Bascule un exemplaire, sauf si c'était le dernier gardé. */
export function toggleMember(group: DupGroupLike, checked: ReadonlySet<number>, id: number): Set<number> {
  const next = new Set(checked);
  if (next.has(id)) next.delete(id);
  else if (canCheck(group, checked, id)) next.add(id);
  return next;
}

/** Coche toutes les copies du groupe sauf l'exemplaire gardé. */
export function checkCopies(group: DupGroupLike, checked: ReadonlySet<number>): Set<number> {
  const next = new Set(checked);
  for (const m of group.members) {
    if (m.id === group.keep) next.delete(m.id);
    else next.add(m.id);
  }
  return next;
}

/** Ce que libère la sélection (taille des exemplaires cochés). */
export function selectedBytes(groups: readonly DupGroupLike[], checked: ReadonlySet<number>): number {
  let total = 0;
  for (const g of groups) for (const m of g.members) if (checked.has(m.id)) total += m.size;
  return total;
}

/** Chaque groupe garde-t-il au moins un exemplaire ? */
export function keepsOneEach(groups: readonly DupGroupLike[], checked: ReadonlySet<number>): boolean {
  return groups.every((g) => g.members.some((m) => !checked.has(m.id)));
}
