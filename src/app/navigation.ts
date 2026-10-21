// Navigation de l'Explorer, comme un navigateur : précédent / suivant sur un historique des
// emplacements visités (zoom, fil d'Ariane, changement de disque).
//
// Un emplacement est retenu par son chemin : les identifiants de nœuds changent d'une analyse à
// l'autre (nouvelle analyse, retour sur un autre disque qu'il faut réanalyser), pas les chemins.

import { create } from "zustand";
import { backend } from "../ipc/client";
import type { NodeId, ScanTarget } from "../ipc/types";
import { startScan } from "./scanning";
import { useStore } from "./store";

export interface Place {
  target: ScanTarget;
  /** Analyse dans laquelle `node` est valable. */
  scanId: number | null;
  node: NodeId | null;
  /** Chemin complet du dossier affiché (null : racine analysée). */
  path: string | null;
  /** Nom affiché dans l'infobulle « Précédent : Local ». */
  label: string;
}

interface NavState {
  back: Place[];
  forward: Place[];
  /** Emplacement affiché (mis à jour par l'Explorer quand la tranche arrive). */
  here: Place | null;
  /** Chemin à rouvrir quand l'analyse du disque demandé sera prête. */
  pending: string | null;
}

/** Au-delà, les plus anciens emplacements sont oubliés. */
const MAX_HISTORY = 100;

export const useNav = create<NavState>()(() => ({ back: [], forward: [], here: null, pending: null }));

const sameTarget = (a: ScanTarget, b: ScanTarget) => JSON.stringify(a) === JSON.stringify(b);

function push(list: Place[], place: Place): Place[] {
  return [...list, place].slice(-MAX_HISTORY);
}

/** L'Explorer signale ce qu'il affiche (après chaque zoom ou analyse). */
export function setHere(place: Place): void {
  const { here } = useNav.getState();
  if (here && here.scanId === place.scanId && here.node === place.node && here.label === place.label && here.path === place.path) return;
  useNav.setState({ here: place });
}

/** Zoom sur un dossier (null : racine analysée) : nouvel emplacement dans l'historique. */
export function goTo(node: NodeId | null): void {
  const { here, back } = useNav.getState();
  if (here && here.node === node) return;
  useNav.setState({ back: here ? push(back, here) : back, forward: [] });
  useStore.getState().zoom(node);
}

/** Changement de disque ou de dossier analysé : nouvel emplacement, puis analyse. */
export function goToTarget(target: ScanTarget): void {
  const { here, back } = useNav.getState();
  if (here && sameTarget(here.target, target)) {
    void startScan(target);
    return;
  }
  useNav.setState({ back: here ? push(back, here) : back, forward: [], pending: null });
  void startScan(target);
}

export function goBack(): void {
  const { back, forward, here } = useNav.getState();
  const place = back.at(-1);
  if (!place) return;
  useNav.setState({ back: back.slice(0, -1), forward: here ? push(forward, here) : forward });
  void restore(place);
}

export function goForward(): void {
  const { back, forward, here } = useNav.getState();
  const place = forward.at(-1);
  if (!place) return;
  useNav.setState({ forward: forward.slice(0, -1), back: here ? push(back, here) : back });
  void restore(place);
}

async function restore(place: Place): Promise<void> {
  const { target, scan, zoom } = useStore.getState();
  if (!sameTarget(target, place.target)) {
    // Autre disque : l'appli ne garde que la dernière analyse, il faut le réanalyser.
    useNav.setState({ pending: place.path });
    await startScan(place.target);
    return;
  }
  const scanId = scan.summary?.scanId ?? null;
  if (place.scanId === scanId) {
    zoom(place.node);
    return;
  }
  zoom(await resolve(scanId, place.path));
}

async function resolve(scanId: number | null, path: string | null): Promise<NodeId | null> {
  if (scanId === null || path === null) return null;
  try {
    const id = await backend.findNode(scanId, path);
    return id !== null && id !== 0 ? id : null;
  } catch {
    return null;
  }
}

/** Fin d'une analyse : rouvre le dossier demandé par « Précédent » / « Suivant », s'il existe encore. */
export async function onScanDone(scanId: number): Promise<void> {
  const { pending } = useNav.getState();
  if (pending === null) return;
  useNav.setState({ pending: null });
  const node = await resolve(scanId, pending);
  if (node !== null) useStore.getState().zoom(node);
}

/** Pour les tests : historique vide. */
export function resetNavigation(): void {
  useNav.setState({ back: [], forward: [], here: null, pending: null });
}
