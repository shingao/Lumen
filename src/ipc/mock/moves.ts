// Déplacement intelligent simulé (navigateur) : vérification avec quelques refus réalistes,
// avancement par étapes, confirmation, journal et suivi des jonctions. Rien ne bouge.
// `localStorage` `lumen-dev-moved=1` : un déplacement déjà fait, disque cible débranché.

import type { Backend } from "../backend";
import type { MoveCheck, MoveProgress, MoveRefusal, MoveRow, MoveRun, MoveStep, NodeId } from "../types";

export interface MockNode {
  name: string;
  path: string;
  size: number;
  files: number;
  dir: boolean;
}

type MovesBackend = Pick<Backend, "moveCheck" | "moveStart" | "moveConfirm" | "moves" | "moveUndo" | "onMoveProgress">;

const GB = 1024 ** 3;
const STEPS: MoveStep[] = ["recheck", "copy", "verify", "setAside", "junction", "test"];

function seeded(): MoveRow[] {
  let on: boolean;
  try {
    on = localStorage.getItem("lumen-dev-moved") === "1";
  } catch {
    on = false;
  }
  if (!on) return [];
  const at = Date.now() - 5 * 86_400_000;
  return [
    {
      id: 1,
      startedAt: at,
      finishedAt: at + 240_000,
      undoneAt: null,
      source: "C:\\Users\\Arthur\\Videos\\Rushes 2025",
      target: "E:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Videos\\Rushes 2025",
      bytes: 48.2 * GB,
      files: 312,
      state: "done",
      step: null,
      message: null,
      health: "targetVolumeAbsent",
    },
  ];
}

export function createMoves(find: (id: NodeId) => MockNode | null): MovesBackend {
  const listeners = new Set<(p: MoveProgress) => void>();
  const rows: MoveRow[] = seeded();
  let nextPlan = 0;
  let nextMove = rows.length;
  const plans = new Map<number, MoveRun>();

  const refusals = (n: MockNode): MoveRefusal[] => {
    const name = n.name.toLowerCase();
    if (!n.dir) return [{ code: "notADirectory", message: "seul un dossier se déplace", apps: [] }];
    if (name.includes("onedrive")) return [{ code: "synced", message: "dossier synchronisé", apps: [] }];
    if (name.includes("discord")) return [{ code: "inUse", message: "ouvert par Discord", apps: ["Discord"] }];
    if (name.includes("steam") || name.includes("jeux")) return [{ code: "gameFolder", message: "dossier de jeu", apps: [] }];
    if (name.includes("docker") || name.includes("wsl")) return [{ code: "virtualDisk", message: "disque virtuel", apps: [] }];
    return [];
  };

  return {
    moveCheck(_scanId, nodeId, drive) {
      const n = find(nodeId);
      if (!n) return Promise.reject(new Error("élément inconnu dans cette analyse"));
      const drives = [
        { letter: "D:", label: "Données", free: 412 * GB },
        { letter: "E:", label: "Archives", free: 0.4 * GB },
      ];
      const chosen = drive ?? "D:";
      const free = drives.find((d) => d.letter === chosen)?.free ?? 0;
      const needed = Math.ceil(n.size * 1.1);
      const target = `${chosen}\\Lumen\\Déplacés\\C\\${n.path.slice(3)}`;
      const r = refusals(n);
      if (free < needed) r.push({ code: "notEnoughSpace", message: "espace insuffisant", apps: [] });
      const ok = r.length === 0;
      const planId = ok ? ++nextPlan : null;
      const check: MoveCheck = {
        planId,
        fingerprint: ok ? "f".repeat(64) : null,
        source: n.path,
        target,
        drive: chosen,
        drives,
        bytes: n.size,
        files: n.files,
        needed,
        free,
        review: n.path.toLowerCase().includes("\\appdata\\"),
        refusals: r,
      };
      if (planId !== null)
        plans.set(planId, { moveId: 0, outcome: "ready", step: null, message: null, refusals: [], source: n.path, target, bytes: n.size, files: n.files });
      return Promise.resolve(check);
    },

    moveStart(planId) {
      const plan = plans.get(planId);
      if (!plan) return Promise.reject(new Error("plan inconnu ou déjà exécuté : rien n'a été fait"));
      plans.delete(planId);
      const moveId = ++nextMove;
      rows.unshift({ id: moveId, startedAt: Date.now(), finishedAt: null, undoneAt: null, source: plan.source, target: plan.target, bytes: plan.bytes, files: plan.files, state: "running", step: "recheck", message: null, health: null });
      return new Promise((resolve) => {
        let i = 0;
        const tick = () => {
          const step = STEPS[Math.min(i >> 2, STEPS.length - 1)] ?? "copy";
          const frac = (i % 4) / 3;
          listeners.forEach((fn) => fn({ moveId, step, done: Math.round(plan.bytes * (step === "copy" || step === "verify" ? frac : 1)), total: plan.bytes }));
          i += 1;
          if (i < STEPS.length * 4) setTimeout(tick, 120);
          else resolve({ ...plan, moveId });
        };
        tick();
      });
    },

    moveConfirm(moveId, deleteOriginal) {
      const row = rows.find((r) => r.id === moveId);
      if (!row) return Promise.reject(new Error("déplacement inconnu ou déjà terminé"));
      row.finishedAt = Date.now();
      row.state = deleteOriginal ? "done" : "rolledBack";
      row.health = deleteOriginal ? "ok" : null;
      return Promise.resolve({ outcome: deleteOriginal ? "done" : "rolledBack", message: null, freed: deleteOriginal ? row.bytes : 0 });
    },

    moves() {
      return Promise.resolve(rows.map((r) => ({ ...r })));
    },

    moveUndo(moveId) {
      const row = rows.find((r) => r.id === moveId);
      if (!row || row.state !== "done") return Promise.resolve({ outcome: "refused", message: "seul un déplacement terminé s'annule" });
      if (row.health === "targetVolumeAbsent") return Promise.resolve({ outcome: "refused", message: "la copie n'est plus sur le disque cible" });
      row.state = "undone";
      row.undoneAt = Date.now();
      row.health = null;
      return Promise.resolve({ outcome: "done", message: null });
    },

    onMoveProgress(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
