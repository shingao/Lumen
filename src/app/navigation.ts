// Navigation de l'Explorer, comme un navigateur : précédent / suivant sur un historique des
// emplacements visités (zoom, fil d'Ariane, changement de disque).
//
// Un emplacement est retenu par son chemin : les identifiants de nœuds changent d'une analyse à
// l'autre (nouvelle analyse, retour sur un autre disque qu'il faut réanalyser), pas les chemins.

import { create } from "zustand";
import { backend } from "../ipc/client";
import type { NodeId, ScanTarget } from "../ipc/types";
import { cancelScan, showCachedScan, startScan } from "./scanning";
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
    // Autre disque : sa dernière analyse de la session s'il y en a une (instantané), sinon
    // une nouvelle analyse qui rouvrira le dossier.
    const cached = await showCachedScan(place.target);
    if (cached !== null) {
      zoom(await resolve(cached, place.path));
      return;
    }
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

/** Ouvre l'Explorer zoomé sur un dossier (chemin complet), depuis un autre écran : dans
 * l'analyse affichée si elle porte sur cette cible, sinon après l'analyse de la cible. Renvoie
 * faux si le dossier n'existe pas dans l'analyse affichée. */
export async function openPath(target: ScanTarget, path: string): Promise<boolean> {
  const s = useStore.getState();
  s.go("analyse");
  const scanId = s.scan.summary?.scanId ?? null;
  if (sameTarget(s.target, target) && scanId !== null && s.scan.status === "done") {
    const node = await resolve(scanId, path);
    if (node === null) return false;
    goTo(node);
    return true;
  }
  goToTarget(target);
  useNav.setState({ pending: path });
  return true;
}

/** Attend la fin d'une analyse en cours (annulée), au plus `ms`. */
function idle(ms: number): Promise<void> {
  return new Promise((done) => {
    if (useStore.getState().scan.status !== "running") return done();
    const timer = setTimeout(() => {
      off();
      done();
    }, ms);
    const off = useStore.subscribe((s) => {
      if (s.scan.status === "running") return;
      clearTimeout(timer);
      off();
      done();
    });
  });
}

/** « Analyser avec Lumen » (Explorateur) ou une notification : l'Explorer sur ce dossier, dans
 * l'analyse de son disque — affichée, gardée de la session ou enregistrée, sinon nouvelle.
 * `path` vient de Rust (canonisé, lecteur connu) ; il ne sert qu'à retrouver le nœud. Renvoie
 * faux si le dossier n'est pas dans l'analyse affichée. */
export async function openFolder(letter: string, path: string): Promise<boolean> {
  const s = useStore.getState();
  s.setSettingsOpen(false);
  s.go("analyse");
  const target: ScanTarget = { kind: "volume", letter };
  const folder = path.replace(/\\+$/, "").length > 2 ? path : null;
  if (sameTarget(s.target, target) && s.scan.status !== "failed") {
    if (s.scan.status === "running") {
      useNav.setState({ pending: folder });
      return true;
    }
    const scanId = s.scan.summary?.scanId ?? null;
    if (folder === null) {
      goTo(null);
      return true;
    }
    const node = await resolve(scanId, folder);
    if (node === null) return false;
    goTo(node);
    return true;
  }
  // Une autre analyse tourne (celle du démarrage) : celle demandée passe avant.
  if (s.scan.status === "running") {
    cancelScan();
    await idle(5_000);
  }
  const { here, back } = useNav.getState();
  useNav.setState({ back: here ? push(back, here) : back, forward: [], pending: null });
  const cached = await showCachedScan(target);
  if (cached !== null) {
    if (folder === null) return true;
    const node = await resolve(cached, folder);
    if (node !== null) useStore.getState().zoom(node);
    return node !== null;
  }
  useNav.setState({ pending: folder });
  await startScan(target);
  return true;
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
