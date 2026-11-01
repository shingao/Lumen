// Mise en évidence dans la treemap (estompage du reste), logique pure. Trois sources, une seule
// active à la fois, dans cet ordre :
//  1. le survol d'une recommandation (seulement pendant le survol) ;
//  2. le panneau de détail ouvert (seulement tant qu'il est ouvert) ;
//  3. le « ? » d'un statut (jusqu'à un second clic sur « ? », Échap ou la puce ✕).
// Les deux dernières, persistantes, sont signalées par une puce au-dessus de la treemap. Si la
// source active ne concerne aucune tuile de la vue, rien n'est estompé.

export type HighlightStatus = "safe" | "review" | "keep";

export interface HighlightNode {
  id: number;
  status: HighlightStatus;
  reco: string | null;
  children: HighlightNode[];
}

export interface RecoLite {
  id: string;
  name: string;
  status: HighlightStatus;
  highlight: readonly number[];
}

export type Source =
  | { kind: "hover"; ids: readonly number[] }
  | { kind: "panel"; reco: string; label: string; ids: readonly number[] }
  | { kind: "status"; status: HighlightStatus; ids: readonly number[] };

/** Source active : survol, sinon panneau ouvert, sinon « ? » d'un statut. */
export function activeSource(
  hover: string | null,
  panel: string | null,
  status: HighlightStatus | null,
  recos: readonly RecoLite[],
): Source | null {
  const find = (id: string | null) => (id === null ? undefined : recos.find((r) => r.id === id));
  const hovered = find(hover);
  if (hovered) return { kind: "hover", ids: hovered.highlight };
  const opened = find(panel);
  if (opened) return { kind: "panel", reco: opened.id, label: opened.name, ids: opened.highlight };
  if (status) return { kind: "status", status, ids: recos.filter((r) => r.status === status).flatMap((r) => r.highlight) };
  return null;
}

/** Ce que la treemap garde net ; `status` : toute tuile de ce statut couverte par une recommandation. */
export interface Keep {
  ids: readonly number[];
  status: HighlightStatus | null;
}

export function keepOf(source: Source): Keep {
  return { ids: source.ids, status: source.kind === "status" ? source.status : null };
}

export function kept(node: HighlightNode, keep: Keep): boolean {
  return keep.ids.includes(node.id) || (keep.status !== null && node.status === keep.status && node.reco !== null);
}

function anyKept(nodes: readonly HighlightNode[], keep: Keep): boolean {
  return nodes.some((n) => kept(n, keep) || anyKept(n.children, keep));
}

export interface InView {
  /** À passer à la treemap : null = rien d'estompé. */
  keep: Keep | null;
  /** La source concerne-t-elle la vue ? (sinon la puce dit « aucun élément ici ») */
  present: boolean;
}

/** La source dans la vue courante : `ancestors` = identifiants du fil d'Ariane (la vue entière est
 * alors dans la zone mise en évidence : rien à estomper). */
export function inView(source: Source | null, root: HighlightNode, ancestors: readonly number[]): InView {
  if (!source) return { keep: null, present: false };
  const keep = keepOf(source);
  if (kept(root, keep) || ancestors.some((a) => keep.ids.includes(a))) return { keep: null, present: true };
  if (anyKept(root.children, keep)) return { keep, present: true };
  return { keep: null, present: false };
}
