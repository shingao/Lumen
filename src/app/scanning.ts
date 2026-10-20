// Analyse : lancement, annulation, suivi des événements du backend.

import { backend, isTauri } from "../ipc/client";
import type { ScanMethod, ScanTarget } from "../ipc/types";
import { useStore } from "./store";

let subscribed = false;

function subscribe(): void {
  if (subscribed) return;
  subscribed = true;
  backend.onScanEvent((e) => {
    const { scan, setScan } = useStore.getState();
    // Une petite analyse peut finir avant que son identifiant ne nous soit revenu.
    const ours = (id: number) => scan.id === id || (scan.id === null && scan.status === "running");
    if (e.type === "progress") {
      if (ours(e.progress.scanId)) setScan({ id: e.progress.scanId, progress: e.progress });
    } else if (e.type === "done") {
      if (!ours(e.summary.scanId)) return;
      useStore.setState({ node: null });
      setScan({ id: e.summary.scanId, status: "done", summary: e.summary, progress: null, message: null });
      // Cochés par défaut : les éléments sans risque seulement (un « À vérifier » ne l'est jamais).
      if (isTauri) {
        void backend
          .recommendations(e.summary.scanId)
          .then((recos) => useStore.getState().setChecked(recos.filter((r) => r.status === "safe").map((r) => r.id)))
          .catch((err: unknown) => console.error("recommandations :", err));
      }
    } else if (ours(e.scanId)) {
      // Annulée ou en échec : on garde l'analyse précédente si elle existe.
      setScan({ status: scan.summary ? "done" : "failed", progress: null, message: e.cancelled ? null : e.message });
    }
  });
}

export async function startScan(target: ScanTarget, method: ScanMethod = "auto"): Promise<void> {
  subscribe();
  const { setScan, setTarget, target: current } = useStore.getState();
  const sameTarget = JSON.stringify(current) === JSON.stringify(target);
  if (!sameTarget) {
    setTarget(target);
    setScan({ summary: null });
  }
  setScan({ status: "running", method, progress: null, message: null, id: null });
  try {
    const id = await backend.startScan(target, method);
    if (useStore.getState().scan.id === null) setScan({ id });
  } catch (err) {
    setScan({ status: useStore.getState().scan.summary ? "done" : "failed", message: String(err) });
  }
}

export function cancelScan(): void {
  const { id } = useStore.getState().scan;
  if (id !== null) void backend.cancelScan(id);
}

export async function initScanning(): Promise<void> {
  subscribe();
  const info = await backend.appInfo().catch(() => null);
  useStore.setState({ appInfo: info });
  await startScan(useStore.getState().target);
}
