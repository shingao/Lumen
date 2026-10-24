// Déplacement intelligent : vérification (refus et programmes qui tiennent le dossier), copie
// vérifiée jusqu'à la jonction testée, puis confirmation explicite de la suppression définitive
// de l'original (ou retour en arrière). Le front n'envoie qu'un id d'analyse, un id de nœud et
// une lettre de disque. Journal des déplacements, annulation et suivi des jonctions.

import { create } from "zustand";
import { formatBytes } from "../core/format";
import { currentT, type Translator } from "../i18n";
import { backend } from "../ipc/client";
import type { MoveCheck, MoveProgress, MoveRefusal, MoveRow, MoveRun, NodeId } from "../ipc/types";
import { useStore } from "./store";

export type MoveStepUi = "closed" | "checking" | "check" | "running" | "ready" | "finishing" | "result";

export interface MoveResult {
  kind: "done" | "rolledBack" | "failed";
  text: string;
}

export interface MoveUi {
  step: MoveStepUi;
  scanId: number | null;
  nodeId: NodeId | null;
  check: MoveCheck | null;
  progress: MoveProgress | null;
  run: MoveRun | null;
  /** « Je confirme la suppression définitive de l'original » coché. */
  ack: boolean;
  result: MoveResult | null;
}

const CLOSED: MoveUi = { step: "closed", scanId: null, nodeId: null, check: null, progress: null, run: null, ack: false, result: null };

export const useMove = create<MoveUi>()(() => CLOSED);
const set = (patch: Partial<MoveUi>) => useMove.setState(patch);

export interface MovesState {
  rows: MoveRow[] | null;
  undoing: number | null;
}

export const useMoves = create<MovesState>()(() => ({ rows: null, undoing: null }));

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

async function check(drive: string | null): Promise<void> {
  const { scanId, nodeId } = useMove.getState();
  if (scanId === null || nodeId === null) return;
  set({ step: "checking" });
  try {
    const c = await backend.moveCheck(scanId, nodeId, drive);
    if (useMove.getState().step === "checking") set({ step: "check", check: c });
  } catch (e) {
    set({ step: "result", result: { kind: "failed", text: message(e) } });
  }
}

/** Ouvre la vérification du déplacement d'un dossier de l'analyse affichée. */
export function openMove(scanId: number, nodeId: NodeId): void {
  useMove.setState({ ...CLOSED, step: "checking", scanId, nodeId });
  void check(null);
}

export function selectDrive(letter: string): void {
  void check(letter);
}

export function closeMove(): void {
  const { step } = useMove.getState();
  // Jamais pendant la copie ni avant la décision sur l'original.
  if (step === "running" || step === "ready" || step === "finishing") return;
  useMove.setState(CLOSED);
}

export function setAck(ack: boolean): void {
  set({ ack });
}

export async function startMove(): Promise<void> {
  const c = useMove.getState().check;
  if (!c || c.planId === null || c.fingerprint === null) return;
  const t = currentT();
  set({ step: "running", progress: null });
  const off = backend.onMoveProgress((p) => set({ progress: p }));
  try {
    const run = await backend.moveStart(c.planId, c.fingerprint);
    if (run.outcome === "ready") set({ step: "ready", run, ack: false });
    else if (run.outcome === "refused")
      set({ step: "check", check: { ...c, planId: null, fingerprint: null, refusals: run.refusals.length ? run.refusals : [{ code: "unreadable", message: run.message ?? "", apps: [] }] } });
    else {
      const step = run.step ? t(`move.step.${run.step}`) : "";
      set({
        step: "result",
        run,
        result: { kind: run.outcome === "rolledBack" ? "rolledBack" : "failed", text: run.outcome === "rolledBack" ? `${t("move.failed", { step, message: run.message ?? "" })} ${t("move.rolledBack")}` : t("move.failedKept", { message: run.message ?? "" }) },
      });
    }
  } catch (e) {
    set({ step: "result", result: { kind: "failed", text: message(e) } });
  } finally {
    off();
    void loadMoves();
  }
}

/** Après la jonction testée : suppression définitive confirmée de l'original, ou retour en arrière. */
export async function confirmMove(deleteOriginal: boolean): Promise<void> {
  const { run, ack } = useMove.getState();
  if (!run || (deleteOriginal && !ack)) return;
  const t = currentT();
  set({ step: "finishing" });
  try {
    const f = await backend.moveConfirm(run.moveId, deleteOriginal);
    const locale = useStore.getState().locale;
    const result: MoveResult =
      f.outcome === "done"
        ? { kind: "done", text: t("move.done", { size: formatBytes(f.freed, locale) }) }
        : f.outcome === "rolledBack"
          ? { kind: "rolledBack", text: t("move.rolledBack") }
          : f.outcome === "deleteFailed"
            ? { kind: "failed", text: t("move.deleteFailed", { message: f.message ?? "" }) }
            : { kind: "failed", text: t("move.failedKept", { message: f.message ?? "" }) };
    set({ step: "result", result });
  } catch (e) {
    set({ step: "result", result: { kind: "failed", text: message(e) } });
  } finally {
    void loadMoves();
  }
}

export async function loadMoves(): Promise<void> {
  try {
    useMoves.setState({ rows: await backend.moves() });
  } catch (e) {
    console.error("déplacements :", e);
    useMoves.setState({ rows: [] });
  }
}

export async function undoMove(id: number): Promise<void> {
  const t = currentT();
  const toast = useStore.getState().showToast;
  useMoves.setState({ undoing: id });
  try {
    const r = await backend.moveUndo(id);
    toast(r.outcome === "done" ? (r.message ? `${t("move.undone")} ${r.message}` : t("move.undone")) : t("move.undoFailed", { message: r.message ?? "" }));
  } catch (e) {
    toast(t("move.undoFailed", { message: message(e) }));
  } finally {
    useMoves.setState({ undoing: null });
    void loadMoves();
  }
}

/** Déplacements faits dont la jonction ou la copie n'est plus joignable (avertissement de l'Analyse). */
export function unhealthy(rows: MoveRow[] | null): MoveRow[] {
  return (rows ?? []).filter((r) => r.state === "done" && r.health !== null && r.health !== "ok");
}

/** Texte d'un refus, avec les programmes qui tiennent le dossier (« Ferme Discord d'abord »). */
export function refusalText(r: MoveRefusal, t: Translator = currentT()): string {
  const key = `move.refusal.${r.code}` as Parameters<Translator>[0];
  const text = t(key, { apps: r.apps.join(", "), message: r.message });
  // Code inconnu de cette version : le message de safety.
  return text === key ? r.message : text;
}
