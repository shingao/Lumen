// Résultats de la recherche (Ctrl K) : Entrée ouvre l'élément (jamais un programme), Ctrl+Entrée
// le montre dans l'Explorateur, Maj+Entrée zoome dessus dans la treemap.

import { currentT } from "../i18n";
import { backend } from "../ipc/client";
import type { ScanTarget, SearchHit } from "../ipc/types";
import { openPath } from "./navigation";
import { useStore } from "./store";

export type HitAction = "open" | "reveal" | "zoom";

function targetOf(hit: SearchHit): ScanTarget {
  return hit.target === "userProfile" ? { kind: "userProfile" } : { kind: "volume", letter: hit.target };
}

export async function runHit(hit: SearchHit, action: HitAction): Promise<void> {
  const store = useStore.getState();
  if (action === "zoom") {
    await openPath(targetOf(hit), hit.path);
    return;
  }
  try {
    const done = await backend.openItem(hit.scanId, hit.id, action);
    if (action === "open" && done === "reveal" && hit.kind === "file") store.showToast(currentT()("palette.revealedInstead"));
  } catch (err) {
    store.showToast(String(err));
  }
}
