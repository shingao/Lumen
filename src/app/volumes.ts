// Volumes présents (puces de l'Explorer) : liste chargée une fois, puis mise à jour à chaud à
// chaque branchement ou débranchement (`volumes:changed`), et après chaque analyse (« analysé il
// y a X »).

import { create } from "zustand";
import { currentT } from "../i18n";
import { backend } from "../ipc/client";
import type { ScanTarget, Volume } from "../ipc/types";
import { useStore } from "./store";

export const useVolumeList = create<{ list: Volume[] | null }>(() => ({ list: null }));

let subscribed = false;

function subscribe(): void {
  if (subscribed) return;
  subscribed = true;
  backend.onVolumesChanged((list) => {
    const before = useVolumeList.getState().list ?? [];
    useVolumeList.setState({ list });
    const t = currentT();
    const name = (v: Volume) => (v.label ? `${v.letter} ${v.label}` : v.letter);
    const added = list.filter((v) => !before.some((b) => b.letter === v.letter));
    const removed = before.filter((b) => !list.some((v) => v.letter === b.letter));
    const { showToast } = useStore.getState();
    if (added[0]) showToast(t("devices.plugged", { name: name(added[0]) }));
    else if (removed[0]) showToast(t("devices.unplugged", { name: name(removed[0]) }));
  });
  backend.onScanEvent((e) => {
    if (e.type === "done") void loadVolumes();
  });
}

export async function loadVolumes(): Promise<Volume[]> {
  subscribe();
  try {
    const list = await backend.volumes();
    useVolumeList.setState({ list });
    return list;
  } catch (err) {
    console.error("volumes :", err);
    return useVolumeList.getState().list ?? [];
  }
}

/** Volume d'une cible (le dossier utilisateur est sur son disque système). */
export function volumeOf(target: ScanTarget, list: readonly Volume[] | null, summaryVolume: string | null): Volume | undefined {
  const letter = target.kind === "volume" ? target.letter : (summaryVolume ?? "C:");
  return list?.find((v) => v.letter.toUpperCase() === letter.toUpperCase());
}

/** Le nettoyage est-il proposé pour la cible affichée ? Non sur une clé USB, une carte SD ou un
 * lecteur réseau (pas de Corbeille) ; le backend le refuse de toute façon. */
export function useCleanable(): boolean {
  const target = useStore((s) => s.target);
  const summaryVolume = useStore((s) => s.scan.summary?.volume ?? null);
  const list = useVolumeList((s) => s.list);
  if (target.kind === "userProfile") return true;
  return volumeOf(target, list, summaryVolume)?.cleanable ?? true;
}
