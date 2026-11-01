import { describe, expect, it } from "vitest";
import { activeSource, type HighlightNode, inView } from "./highlight";

const n = (id: number, status: HighlightNode["status"], reco: string | null, children: HighlightNode[] = []): HighlightNode => ({ id, status, reco, children });
const recos = [
  { id: "pip", name: "Cache pip", status: "safe" as const, highlight: [11] },
  { id: "android", name: "Android SDK", status: "review" as const, highlight: [21] },
];

describe("mise en évidence : une seule source, rien d'estompé hors de la vue", () => {
  it("survol, puis panneau, puis « ? » : une seule source à la fois", () => {
    expect(activeSource("android", "pip", "safe", recos)).toEqual({ kind: "hover", ids: [21] });
    expect(activeSource(null, "pip", "safe", recos)).toEqual({ kind: "panel", reco: "pip", label: "Cache pip", ids: [11] });
    expect(activeSource(null, null, "review", recos)).toEqual({ kind: "status", status: "review", ids: [21] });
    expect(activeSource(null, null, null, recos)).toBeNull();
    // Recommandation disparue (autre analyse) : aucune source.
    expect(activeSource(null, "fantome", null, recos)).toBeNull();
  });

  it("source présente dans la vue : estompage ; absente : rien (« aucun élément ici »)", () => {
    const local = n(1, "keep", null, [n(11, "safe", "pip"), n(12, "keep", null)]);
    const pip = activeSource(null, "pip", null, recos);
    expect(inView(pip, local, [])).toEqual({ keep: { ids: [11], status: null }, present: true });
    // AppData\Roaming : aucune tuile pip.
    const roaming = n(2, "keep", null, [n(31, "keep", null), n(32, "keep", null)]);
    expect(inView(pip, roaming, [])).toEqual({ keep: null, present: false });
    // Zoom dans la zone mise en évidence : toute la vue en fait partie, rien à estomper.
    expect(inView(pip, n(11, "safe", "pip", [n(111, "safe", "pip")]), [0, 1, 11])).toEqual({ keep: null, present: true });
    // « ? » d'un statut : toute tuile de ce statut couverte par une recommandation, même profonde.
    const status = activeSource(null, null, "review", recos);
    expect(inView(status, n(1, "keep", null, [n(5, "keep", null, [n(51, "review", "android")])]), [])).toEqual({ keep: { ids: [21], status: "review" }, present: true });
    expect(inView(null, local, [])).toEqual({ keep: null, present: false });
  });
});
