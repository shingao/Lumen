// Nettoyage : plan validé par safety → confirmation → exécution réelle → bilan → journal.
// Rien n'est fait sans la confirmation du plan affiché (son empreinte) ; le définitif demande
// en plus une saisie (le total ou « SUPPRIMER »).

import { backend } from "../ipc/client";
import type { CleanMode, CleanRequest } from "../ipc/types";
import { CLEAN_CLOSED, useStore } from "./store";

let subscribed = false;

function subscribe(): void {
  if (subscribed) return;
  subscribed = true;
  backend.onCleanEvent((e) => {
    const { clean, setClean, bumpJournal } = useStore.getState();
    const ours = (planId: number) => clean.plan?.planId === planId;
    if (e.type === "progress") {
      if (ours(e.progress.planId)) setClean({ progress: e.progress });
    } else if (e.type === "done") {
      if (!ours(e.report.planId)) return;
      setClean({ step: "report", report: e.report, progress: null, cancelling: false });
      bumpJournal();
    } else if (ours(e.planId)) {
      setClean({ step: "error", message: e.message, progress: null, cancelling: false });
    }
  });
}

async function plan(request: CleanRequest, mode: CleanMode): Promise<void> {
  const { scan, setClean } = useStore.getState();
  // Les gouffres système ne dépendent d'aucune analyse.
  const scanId = scan.summary?.scanId ?? (request.kind === "system" ? 0 : undefined);
  if (scanId === undefined) return;
  setClean({ step: "loading", request, mode, plan: null, report: null, message: null, waiveRestorePoint: false });
  try {
    const p = await backend.planCleanup(scanId, request, mode);
    // Une autre demande a pu remplacer celle-ci entre-temps.
    const now = useStore.getState().clean;
    if (now.step === "loading" && now.mode === mode && now.request === request) setClean({ step: "confirm", plan: p });
  } catch (err) {
    setClean({ step: "error", message: String(err) });
  }
}

/** Ouvre la confirmation pour les recommandations données (Corbeille par défaut). */
export function openCleanup(ids: string[]): void {
  if (ids.length === 0) return;
  openRequest({ kind: "recos", ids });
}

/** Ouvre la confirmation pour une sélection du nettoyage avancé (identifiants seulement). */
export function openRequest(request: CleanRequest): void {
  subscribe();
  void plan(request, "trash");
}

/** Corbeille ↔ définitif (seulement si les réglages l'autorisent) : nouveau plan, nouvelle empreinte. */
export function setCleanMode(mode: CleanMode): void {
  const { clean, settings } = useStore.getState();
  if (mode === "permanent" && !settings?.permanentDelete) return;
  if (clean.request) void plan(clean.request, mode);
}

/** Premier bouton : Corbeille → exécution ; définitif → seconde confirmation. */
export function confirmCleanup(): void {
  const { clean, setClean } = useStore.getState();
  if (clean.step !== "confirm" || !clean.plan) return;
  // Actions système sans point de restauration : seulement après la confirmation explicite.
  if (clean.plan.withoutRestorePoint && !clean.waiveRestorePoint) return;
  if (clean.plan.mode === "permanent") {
    setClean({ step: "permanent" });
    return;
  }
  void execute(false);
}

/** La saisie attendue pour le définitif : le total affiché, ou « SUPPRIMER » (« DELETE »). */
export function permanentPhraseOk(input: string, total: string): boolean {
  const norm = (s: string) => s.replace(/[\s\u00a0\u202f]+/g, "").toUpperCase();
  const typed = norm(input);
  return typed.length > 0 && (typed === norm(total) || typed === "SUPPRIMER" || typed === "DELETE");
}

export function confirmPermanent(): void {
  const { clean } = useStore.getState();
  if (clean.step !== "permanent") return;
  void execute(true);
}

async function execute(permanentAcknowledged: boolean): Promise<void> {
  const { clean, setClean } = useStore.getState();
  const p = clean.plan;
  if (!p) return;
  setClean({ step: "running", progress: null, cancelling: false });
  try {
    await backend.executePlan(p.planId, p.fingerprint, permanentAcknowledged, p.withoutRestorePoint && clean.waiveRestorePoint);
  } catch (err) {
    setClean({ step: "error", message: String(err) });
  }
}

/** Case « Continuer sans point de restauration ». */
export function setWaiveRestorePoint(on: boolean): void {
  useStore.getState().setClean({ waiveRestorePoint: on });
}

/** Point de restauration impossible : nouveau plan des mêmes actions, sans lui ; il faudra cocher
 * « Continuer sans point de restauration » pour le lancer. */
export function continueWithoutRestorePoint(): void {
  const { clean } = useStore.getState();
  if (clean.request?.kind !== "system") return;
  void plan({ ...clean.request, restorePoint: false }, "trash");
}

/** Annulation prise en compte entre deux éléments ; sinon ferme sans rien faire. */
export function cancelCleanup(): void {
  const { clean, setClean } = useStore.getState();
  if (clean.step === "running" && clean.plan) {
    setClean({ cancelling: true });
    void backend.cancelPlan(clean.plan.planId);
    return;
  }
  if (clean.step === "permanent") {
    setClean({ step: "confirm" });
    return;
  }
  closeCleanup();
}

export function closeCleanup(): void {
  const { clean } = useStore.getState();
  if (clean.step === "running") return;
  useStore.setState({ clean: CLEAN_CLOSED });
}
