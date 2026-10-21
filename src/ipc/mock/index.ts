// Backend factice : sert les données de la maquette avec la même forme que le futur backend Rust.

import { targetKey, type Backend } from "../backend";
import type { Locale } from "../../core/format";
import { GIB } from "../../core/format";
import { fuzzyFilter } from "../../core/fuzzy";
import type {
  CleanEvent,
  CleanPlan,
  CleanReport,
  CleanKind,
  Crumb,
  DuplicatesEvent,
  JournalAction,
  JournalDetail,
  Kpis,
  NodeId,
  Recommendation,
  RecommendationDetail,
  ScanEvent,
  ScanSummary,
  SearchHit,
  Settings,
  TreeNode,
  TreeSlice,
  Volume,
} from "../types";
import type { L10n, MockReco, MockTile, When } from "./data";
import { D_TILES, KPI, RECOS, TILES, USER_ROOT, VOLUMES } from "./data";
import { createMockTimeline } from "./timeline";
import { DEV_EXTRA_CACHES, mockCover, mockDevCaches, mockDocker, mockGames, mockProjects, mockRunning, mockShaders, mockWsl } from "./devgames";
import { mockAdvancedPlan, mockDormant, mockDuplicates, mockLargest, mockNoProtection, mockProgram, mockRestorePoints, mockSinks } from "./advanced";

const SETTINGS_KEY = "lumen-dev-settings";
/** Durée d'une étape du nettoyage simulé. */
const MOCK_STEP_MS = 120;

export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  language: "system",
  excludedFolders: [],
  dormantMonths: 6,
  duplicateMinMb: 1,
  permanentDelete: false,
  dailySnapshot: true,
  trayIcon: false,
  lowSpaceAlert: true,
  lowSpaceThreshold: 10,
  explorerMenu: true,
  logsFolder: "%LOCALAPPDATA%\\Lumen\\logs",
  experimentalSystem: false,
  experimentalWsl: false,
};

const gb = (n: number) => Math.round(n * GIB);

function resolveWhen(when: When, now: number): number {
  if ("msAgo" in when) return now - when.msAgo;
  const d = new Date(now);
  d.setDate(d.getDate() - when.daysAgo);
  if (when.at) d.setHours(when.at[0], when.at[1], 0, 0);
  return d.getTime();
}

interface VolumeTree {
  rootId: NodeId;
  /** Segments au-dessus de la racine analysée (non cliquables) puis la racine. */
  prefix: string[];
  base: string;
  nodes: Map<NodeId, { tile: MockTile | null; parent: NodeId | null; children: NodeId[] }>;
  byKey: Map<string, NodeId>;
}

function buildVolume(tiles: MockTile[], rootId: NodeId, prefix: string[], base: string): VolumeTree {
  const nodes: VolumeTree["nodes"] = new Map();
  const byKey = new Map<string, NodeId>();
  nodes.set(rootId, { tile: null, parent: null, children: [] });
  tiles.forEach((tile, i) => byKey.set(tile.id, rootId + 1 + i));
  tiles.forEach((tile) => {
    const id = byKey.get(tile.id) as NodeId;
    const parent = tile.parent ? (byKey.get(tile.parent) as NodeId) : rootId;
    nodes.set(id, { tile, parent, children: [] });
  });
  tiles.forEach((tile) => {
    const id = byKey.get(tile.id) as NodeId;
    const parent = nodes.get(id)?.parent;
    if (parent !== undefined && parent !== null) nodes.get(parent)?.children.push(id);
  });
  return { rootId, prefix, base, nodes, byKey };
}

const TREES: Record<string, VolumeTree> = {
  "C:": buildVolume(TILES, 0, [USER_ROOT.volume, ...USER_ROOT.segments], "C:\\Users\\Arthur\\"),
  "D:": buildVolume(D_TILES, 10_000, ["D:"], "D:\\"),
};

function readSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<Settings>) };
  } catch {
    // Stockage indisponible : réglages par défaut.
  }
  return { ...DEFAULT_SETTINGS };
}

function writeSettings(s: Settings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Non persistant dans ce contexte : sans conséquence pour la démo.
  }
}

export function createMockBackend(now: () => number = Date.now): Backend {
  let locale: Locale = "fr";
  let settings = readSettings();
  const timeline = createMockTimeline(now);
  const l = (v: L10n | string) => (typeof v === "string" ? v : v[locale]);

  const treeOf = (volume: string): VolumeTree => TREES[volume] ?? (TREES["C:"] as VolumeTree);

  function sizeOf(tree: VolumeTree, id: NodeId): number {
    const n = tree.nodes.get(id);
    if (!n) return 0;
    if (n.tile) return n.tile.size;
    return n.children.reduce((sum, c) => sum + sizeOf(tree, c), 0);
  }

  /** Même règle que le backend : enfants et petits-enfants toujours, puis jusqu'à 4 niveaux sous
   * les dossiers d'au moins 1 % de la vue. */
  function toNode(tree: VolumeTree, id: NodeId, level: number, minExpand: number): TreeNode {
    const n = tree.nodes.get(id);
    const t = n?.tile ?? null;
    const expand = level < 2 || (level < 4 && sizeOf(tree, id) >= minExpand);
    const children = expand ? (n?.children ?? []).map((c) => toNode(tree, c, level + 1, minExpand)) : [];
    if (!t) {
      const name = tree.prefix[tree.prefix.length - 1] ?? "";
      return {
        id,
        name,
        path: tree.base.replace(/\\$/, ""),
        kind: "dir",
        size: gb(sizeOf(tree, id)),
        logical: gb(sizeOf(tree, id) * 0.97),
        othersCount: null,
        files: children.reduce((s, c) => s + c.files, 0),
        lastOpened: Math.max(0, ...children.map((c) => c.lastOpened)),
        category: "system",
        status: "keep",
        reco: null,
        gain: 0,
        gainStatus: null,
        gainUpTo: false,
        hasChildren: (n?.children.length ?? 0) > 0,
        children,
      };
    }
    return {
      id,
      name: l(t.name),
      path: tree.base + l(t.path),
      kind: (n?.children.length ?? 0) > 0 || t.files > 1 ? "dir" : "file",
      size: gb(t.size),
      logical: gb(t.size * 0.97),
      othersCount: null,
      ...(t.layoutSize !== undefined ? { layoutSize: gb(t.layoutSize) } : {}),
      files: t.files,
      lastOpened: resolveWhen(t.last, now()),
      category: t.category,
      status: t.status,
      reco: t.reco,
      gain: gb(t.gain ?? 0),
      gainStatus: t.gainStatus ?? null,
      gainUpTo: t.gainUpTo ?? false,
      hasChildren: (n?.children.length ?? 0) > 0,
      children,
    };
  }

  function breadcrumb(tree: VolumeTree, id: NodeId): Crumb[] {
    const chain: Crumb[] = [];
    let cur: NodeId | null = id;
    while (cur !== null && cur !== tree.rootId) {
      const n = tree.nodes.get(cur);
      if (!n?.tile) break;
      chain.unshift({ label: l(n.tile.name), id: cur });
      cur = n.parent;
    }
    const above = tree.prefix.slice(0, -1).map((label) => ({ label, id: null }));
    return [...above, { label: tree.prefix[tree.prefix.length - 1] ?? "", id: tree.rootId }, ...chain];
  }

  const recosOf = (volume: string): MockReco[] => (volume === "C:" ? RECOS : []);

  // Analyses factices : chaque cible pointe vers un jeu de données de la maquette.
  const scans = new Map<number, string>();
  /** Analyses terminées (la dernière de chaque cible est gardée, comme dans l'appli). */
  const finished = new Set<number>();
  let nextScan = 0;
  const listeners = new Set<(e: ScanEvent) => void>();
  const emit = (e: ScanEvent) => listeners.forEach((fn) => fn(e));
  const volumeOf = (key: string) => (key === "D:" ? "D:" : "C:");
  const scanVolume = (scanId: number) => volumeOf(scans.get(scanId) ?? "user");
  const summaryOf = (scanId: number): ScanSummary => {
    const tree = treeOf(scanVolume(scanId));
    const files = [...tree.nodes.values()].reduce((s, n) => s + (n.tile && !n.tile.parent ? n.tile.files : 0), 0);
    return {
      scanId,
      rootPath: tree.base.replace(/\\$/, ""),
      method: "walk",
      files,
      dirs: Math.round(files / 12),
      allocated: gb(sizeOf(tree, tree.rootId)),
      logical: gb(sizeOf(tree, tree.rootId) * 0.97),
      links: 12,
      cloudFiles: 3412,
      cloudDirs: 2,
      cloudBytes: gb(6.4),
      hardlinkDups: 0,
      accessDenied: 4,
      locked: 1,
      otherErrors: 0,
      elapsedMs: 2300,
      lastAccessReliable: false,
      notice: null,
      volume: scanVolume(scanId),
    };
  };

  function toReco(r: MockReco): Recommendation {
    const tree = TREES["C:"] as VolumeTree;
    return {
      id: r.id,
      status: r.status,
      name: l(r.name),
      path: l(r.path),
      size: gb(r.size),
      highlight: r.highlight.map((k) => tree.byKey.get(k)).filter((v): v is NodeId => v !== undefined),
    };
  }

  function scriptLines(r: MockReco, whatIf: boolean): string[] {
    return r.script.map((line) => l(line).replace("§", whatIf ? " -WhatIf" : ""));
  }

  const notes = {
    safe: {
      fr: "Corbeille par défaut : rien n'est supprimé définitivement sans ta double confirmation.",
      en: "Recycle Bin by default: nothing is permanently deleted without your double confirmation.",
    },
    review: {
      fr: "Rien ne part sans ta confirmation. Relis la liste avant de lancer le script.",
      en: "Nothing leaves without your confirmation. Read the list before running the script.",
    },
  };

  const resolved = <T>(value: T): Promise<T> => Promise.resolve(value);
  const NO_PROTECTION = {
    fr: "Windows ne peut pas créer de point de restauration sur ce disque : la protection du système y est désactivée",
    en: "Windows cannot create a restore point on this disk: system protection is turned off for it",
  };

  // Nettoyage simulé : même déroulé que l'appli (plan, confirmation, avancement, bilan, journal).
  const plans = new Map<number, CleanPlan>();
  const cancelled = new Set<number>();
  let nextPlan = 0;
  const cleanListeners = new Set<(e: CleanEvent) => void>();
  const emitClean = (e: CleanEvent) => cleanListeners.forEach((fn) => fn(e));
  const journalActions: JournalAction[] = [];
  const journalDetails = new Map<number, JournalDetail>();
  const planKinds = new Map<number, CleanKind>();
  // Doublons simulés : avancement par étapes, annulable.
  const dupListeners = new Set<(e: DuplicatesEvent) => void>();
  const emitDup = (e: DuplicatesEvent) => dupListeners.forEach((fn) => fn(e));
  const dupCancelled = new Set<number>();

  return {
    setLocale(next) {
      locale = next;
    },

    appInfo() {
      return resolved({ elevated: false, fastScanAvailable: true, version: "0.1.0" });
    },

    volumes() {
      return resolved<Volume[]>(
        VOLUMES.map((v) => ({
          id: v.id,
          letter: v.letter,
          label: l(v.label),
          kind: v.kind,
          fileSystem: v.kind === "network" ? "SMB" : "NTFS",
          mftCapable: v.kind !== "network",
          total: gb(v.total),
          free: gb(v.free),
        })),
      );
    },

    startScan(target) {
      const id = ++nextScan;
      scans.set(id, targetKey(target));
      const total = summaryOf(id);
      // Progression rapide puis résultat, comme le backend réel (événements ≈ 10/s).
      [0.25, 0.6, 0.9].forEach((ratio, i) => {
        setTimeout(() => {
          emit({
            type: "progress",
            progress: {
              scanId: id,
              files: Math.round(total.files * ratio),
              dirs: Math.round(total.dirs * ratio),
              allocated: Math.round(total.allocated * ratio),
              errors: 0,
              elapsedMs: (i + 1) * 100,
              phase: "walk",
              tops: [],
            },
          });
        }, (i + 1) * 100);
      });
      setTimeout(() => {
        finished.add(id);
        emit({ type: "done", summary: total });
      }, 400);
      return resolved(id);
    },

    cachedScan(target) {
      const key = targetKey(target);
      const ids = [...finished].filter((id) => scans.get(id) === key);
      const last = ids.length > 0 ? Math.max(...ids) : null;
      return resolved(last === null ? null : summaryOf(last));
    },

    cancelScan(scanId) {
      emit({ type: "failed", scanId, cancelled: true, message: "analyse annulée" });
      return resolved(undefined);
    },

    onScanEvent(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    kpis(target) {
      const volume = volumeOf(targetKey(target));
      const recos = recosOf(volume);
      const sum = (status: string) => recos.filter((r) => r.status === status).reduce((s, r) => s + r.size, 0);
      const count = (status: string) => recos.filter((r) => r.status === status).length;
      const vol = VOLUMES.find((v) => v.id === volume);
      const freeNow = volume === "C:" ? KPI.freeNow : (vol?.free ?? 0);
      const k: Kpis = {
        rulesReady: true,
        freeNow: gb(freeNow),
        freeAfter: gb(freeNow + sum("safe")),
        safeBytes: gb(sum("safe")),
        safeCount: count("safe"),
        reviewBytes: gb(sum("review")),
        reviewCount: count("review"),
        unanalyzed: gb(volume === "C:" ? KPI.unanalyzed : 0),
        trend7d: timeline.trend(target) ?? 0,
      };
      return resolved(k);
    },

    tree(scanId, node) {
      const tree = treeOf(scanVolume(scanId));
      const id = node !== null && tree.nodes.has(node) ? node : tree.rootId;
      const slice: TreeSlice = { root: toNode(tree, id, 0, sizeOf(tree, id) / 100), breadcrumb: breadcrumb(tree, id) };
      return resolved(slice);
    },

    findNode(scanId, path) {
      const tree = treeOf(scanVolume(scanId));
      const want = path.replace(/\\$/, "").toLowerCase();
      if (tree.base.replace(/\\$/, "").toLowerCase() === want) return resolved(tree.rootId);
      for (const [id, n] of tree.nodes) {
        if (n.tile && (tree.base + l(n.tile.path)).toLowerCase() === want) return resolved(id);
      }
      return resolved(null);
    },

    recommendations(scanId) {
      return resolved(recosOf(scanVolume(scanId)).map(toReco));
    },

    recommendation(id) {
      const r = RECOS.find((x) => x.id === id);
      if (!r) return Promise.reject(new Error(`recommandation inconnue : ${id}`));
      const detail: RecommendationDetail = {
        ...toReco(r),
        explanation: l(r.explanation),
        meta: l(r.meta),
        files: r.files.map(([path, size]) => ({ path: l(path), size: gb(size) })),
        script: r.script.map((line) => {
          const text = l(line);
          const at = text.indexOf("§");
          return { text: text.replace("§", ""), whatIfAt: at < 0 ? null : at };
        }),
        note: l(notes[r.status]),
      };
      return resolved(detail);
    },

    script(_scanId, ids) {
      const parts = RECOS.filter((r) => ids.includes(r.id)).map((r) => scriptLines(r, true).join("\n"));
      return resolved(parts.join("\n\n"));
    },

    planCleanup(_scanId, request, mode) {
      if (request.kind !== "recos") {
        const { items, commands } = mockAdvancedPlan(request, locale === "en", now());
        const total = items.reduce((a, i) => a + i.size, 0) + commands.reduce((a, c) => a + c.size, 0);
        const plan: CleanPlan = {
          planId: ++nextPlan,
          fingerprint: (nextPlan * 2654435761).toString(16).padStart(64, "0"),
          mode,
          total,
          count: items.length + commands.filter((c) => !c.prerequisite).length,
          items,
          moreItems: 0,
          commands,
          volumes: [{ volume: "C:\\", free: gb(KPI.freeNow) }],
          refused: [],
          skippedLocked: 0,
          elevated: request.kind === "system" || request.kind === "wslCompact",
          withoutRestorePoint: request.kind === "system" && !request.restorePoint,
          processGuard: request.kind === "shaders",
        };
        plans.set(plan.planId, plan);
        planKinds.set(plan.planId, request.kind === "dockerPrune" ? "docker" : request.kind === "wslCompact" ? "wsl" : request.kind);
        return resolved(plan);
      }
      const ids = request.ids;
      const chosen = RECOS.filter((r) => ids.includes(r.id));
      const commands = chosen
        .filter((r) => r.id === "npm")
        .flatMap((r) => [
          { line: "npm cache clean --force", explanation: l({ fr: "Outil officiel : vide le cache npm", en: "Official tool: clears the npm cache" }), prerequisite: false, required: false, needsElevation: false, size: gb(r.size) },
          { line: "npm cache verify", explanation: l({ fr: "Vérifie l'intégrité du cache npm", en: "Checks the npm cache integrity" }), prerequisite: false, required: false, needsElevation: false, size: 0 },
        ]);
      const extra = DEV_EXTRA_CACHES.filter((c) => ids.includes(c.id));
      for (const c of extra.filter((x) => x.line)) {
        commands.push({ line: c.line ?? "", explanation: l({ fr: `Outil officiel : vide le ${c.fr.toLowerCase()}`, en: `Official tool: clears the ${c.en}` }), prerequisite: false, required: false, needsElevation: false, size: gb(c.size) });
      }
      const items = chosen
        .filter((r) => r.id !== "npm")
        .flatMap((r) => r.files.map(([path, size]) => ({ path: l(path), size: gb(size), directory: true, rule: r.id, ruleName: l(r.name) })))
        .concat(extra.filter((c) => !c.line).map((c) => ({ path: `C:\\Users\\Arthur${c.path.slice(1)}`, size: gb(c.size), directory: true, rule: c.id, ruleName: l(c) })));
      const total = items.reduce((a, i) => a + i.size, 0) + commands.reduce((a, c) => a + c.size, 0);
      const plan: CleanPlan = {
        planId: ++nextPlan,
        fingerprint: (nextPlan * 2654435761).toString(16).padStart(64, "0"),
        mode,
        total,
        count: items.length + commands.filter((c) => c.size > 0 || !c.prerequisite).length,
        items,
        moreItems: 0,
        commands,
        volumes: [{ volume: "C:\\", free: gb(KPI.freeNow) }],
        refused: [],
        skippedLocked: 0,
        elevated: false,
        withoutRestorePoint: false,
        processGuard: false,
      };
      plans.set(plan.planId, plan);
      planKinds.set(plan.planId, "recos");
      return resolved(plan);
    },

    executePlan(planId, fingerprint, permanentAcknowledged, withoutRestorePoint) {
      const plan = plans.get(planId);
      if (!plan || plan.fingerprint !== fingerprint) return Promise.reject(new Error("le plan a changé depuis sa confirmation : rien n'a été fait"));
      if (plan.mode === "permanent" && !permanentAcknowledged) return Promise.reject(new Error("la suppression définitive demande une seconde confirmation"));
      if (plan.withoutRestorePoint && !withoutRestorePoint) {
        return Promise.reject(new Error("actions système sans point de restauration : il faut confirmer « Continuer sans point de restauration »"));
      }
      // Protection du système désactivée : le point de restauration échoue, rien d'autre n'est lancé.
      const blocked = plan.elevated && mockNoProtection() && plan.commands.some((c) => c.required);
      plans.delete(planId);
      cancelled.delete(planId);
      const steps = [...plan.commands.map((c) => ({ label: c.line, size: c.size, command: true })), ...plan.items.map((i) => ({ label: i.path, size: i.size, command: false }))];
      const started = Date.now();
      let freed = 0;
      let index = 0;
      const finish = (wasCancelled: boolean) => {
        const done = blocked ? 0 : Math.min(index, plan.items.length + plan.commands.length);
        const report: CleanReport = {
          planId,
          actionId: journalActions.length + 1,
          status: wasCancelled ? "cancelled" : blocked ? "failed" : "ok",
          mode: plan.mode,
          freed,
          measured: freed,
          done: Math.max(0, done - plan.commands.length),
          partial: 0,
          skipped: 0,
          refused: 0,
          failed: 0,
          cancelled: plan.items.length - Math.max(0, done - plan.commands.length),
          items: plan.items.map((i, k) => ({
            path: i.path,
            planned: i.size,
            freed: k < done - plan.commands.length ? i.size : 0,
            outcome: k < done - plan.commands.length ? "done" : "cancelled",
            reason: null,
            issues: [],
            moreIssues: 0,
          })),
          // Point de restauration : Windows en garde un par 24 h, celui de moins de 24 h compte.
          commands: plan.commands.map((c, k) =>
            blocked && c.required
              ? { line: c.line, status: "failed", satisfied: false, detail: l(NO_PROTECTION), log: null }
              : c.required && k < done
                ? { line: c.line, status: "failed", satisfied: true, detail: l({ fr: "un point de restauration existe déjà depuis moins de 24 h (limite de Windows) : il sert de point de retour", en: "a restore point already exists from the last 24 h (Windows limit): it is used as the fallback" }), log: null }
                : { line: c.line, status: k < done ? "succeeded" : "cancelled", satisfied: k < done, detail: null, log: c.line.includes("/LogPath:") ? (c.line.split("/LogPath:")[1] ?? null) : null },
          ),
          aborted: blocked ? l(NO_PROTECTION) : null,
          restorePointFailed: blocked,
          journalError: null,
        };
        const action: JournalAction = {
          id: report.actionId ?? 0,
          startedAt: started,
          finishedAt: Date.now(),
          title: [...new Set(plan.items.map((i) => i.ruleName))].join(", ") || plan.commands.map((c) => c.line).join(", "),
          method: plan.mode,
          status: report.status,
          bytesPlanned: plan.total,
          bytesFreed: freed,
          bytesMeasured: freed,
          itemsDone: report.done,
          itemsPartial: 0,
          itemsSkipped: 0,
          itemsRefused: 0,
          itemsFailed: 0,
          itemsCancelled: report.cancelled,
          kind: planKinds.get(planId) ?? "recos",
          aborted: report.aborted,
          withoutRestorePoint: plan.withoutRestorePoint,
        };
        journalActions.unshift(action);
        journalDetails.set(action.id, {
          action,
          items: report.items.map((i, k) => ({
            id: action.id * 1000 + k,
            path: i.path,
            ruleId: plan.items[k]?.rule ?? null,
            directory: true,
            size: i.planned,
            method: plan.mode,
            result: i.outcome === "done" ? "done" : "cancelled",
            reason: null,
            restoredAt: null,
            restorable: i.outcome === "done" && plan.mode === "trash",
          })),
          commands: plan.commands.map((c, k) => ({
            role: c.prerequisite ? "prerequisite" : "action",
            program: mockProgram(c.line),
            args: c.line.split(" ").slice(1),
            status: c.required && k < done ? "failed" : k < done ? "succeeded" : "cancelled",
            exitCode: c.required && k < done ? 3 : k < done ? 0 : null,
            stdout: "",
            stderr: "",
            durationMs: 1200,
            logPath: report.commands[k]?.log ?? null,
            logTail: report.commands[k]?.log ? "[==========================100.0%==========================]\nL'opération a réussi." : null,
          })),
        });
        emitClean({ type: "done", report });
      };
      const step = () => {
        if (blocked || cancelled.has(planId) || index >= steps.length) {
          finish(cancelled.has(planId));
          return;
        }
        const s = steps[index] as (typeof steps)[number];
        const commandsLeft = index < plan.commands.length;
        emitClean({
          type: "progress",
          progress: { planId, phase: s.command ? "command" : "item", index, total: commandsLeft ? plan.commands.length : plan.items.length, label: s.label, verified: 0, freed },
        });
        setTimeout(() => {
          freed += s.size;
          index += 1;
          step();
        }, MOCK_STEP_MS);
      };
      setTimeout(step, 0);
      return resolved(undefined);
    },

    cancelPlan(planId) {
      cancelled.add(planId);
      return resolved(undefined);
    },

    onCleanEvent(listener) {
      cleanListeners.add(listener);
      return () => cleanListeners.delete(listener);
    },

    dormant: (_scanId, months) => resolved(mockDormant(months, now())),
    largest: (_scanId, kind) => resolved(mockLargest(kind, now())),

    startDuplicates(scanId) {
      dupCancelled.delete(scanId);
      const steps: ["edges" | "full", number, number][] = [
        ["edges", 120, 640],
        ["edges", 400, 640],
        ["edges", 640, 640],
        ["full", 6, 18],
        ["full", 18, 18],
      ];
      steps.forEach(([phase, done, total], i) => {
        setTimeout(() => {
          if (!dupCancelled.has(scanId)) emitDup({ type: "progress", progress: { scanId, phase, done, total } });
        }, (i + 1) * 150);
      });
      setTimeout(() => {
        if (dupCancelled.has(scanId)) emitDup({ type: "failed", scanId, cancelled: true, message: "recherche annulée" });
        else emitDup({ type: "done", result: mockDuplicates(scanId, now()) });
      }, (steps.length + 1) * 150);
      return resolved(undefined);
    },

    cancelDuplicates(scanId) {
      dupCancelled.add(scanId);
      return resolved(undefined);
    },

    onDuplicatesEvent(listener) {
      dupListeners.add(listener);
      return () => dupListeners.delete(listener);
    },

    games: () => resolved(mockGames(now())),
    gameCover: (id) => resolved(mockCover(id)),
    openGame: () => resolved(undefined),
    gamesRunning: () => resolved(mockRunning()),
    shaderCaches: (scanId) => resolved(mockShaders(scanId)),
    devOverview(_scanId) {
      const base = RECOS.filter((r) => ["npm", "gradle", "pip"].includes(r.id)).map((r) => ({ id: r.id, status: r.status, name: l(r.name), path: l(r.path), size: gb(r.size), command: r.id === "npm" }));
      return resolved({ projects: mockProjects(now()), caches: mockDevCaches(base, locale === "en") });
    },
    dockerImages: () => resolved(mockDocker()),
    wslDisks: () => resolved(mockWsl()),
    timelineSeries: () => resolved(timeline.series()),
    timeline: (series) => resolved(timeline.data(series)),
    timelineTrend: (target) => resolved(timeline.trend(target)),
    timelineCompare(from, to) {
      try {
        return resolved(timeline.compare(from, to));
      } catch (e) {
        return Promise.reject(e instanceof Error ? e : new Error(String(e)));
      }
    },
    timelineDaily: (enabled) => resolved(timeline.daily(enabled)),
    onTimelineChanged: (listener) => timeline.onChanged(listener),
    database: () => resolved(timeline.database()),
    purgeSnapshots: () => resolved(timeline.purge()),
    systemSinks: () => resolved(mockSinks()),
    readRestorePoints: () => resolved(mockRestorePoints(now())),
    openSystemProtection: () => resolved(undefined),

    journal: () => resolved([...journalActions]),

    journalDetail(actionId) {
      const d = journalDetails.get(actionId);
      return d ? resolved(structuredClone(d)) : Promise.reject(new Error("entrée du journal inconnue"));
    },

    journalScript: (actionId) => resolved(`# Lumen · ${journalDetails.get(actionId)?.action.title ?? ""}`),

    restore(itemId) {
      for (const d of journalDetails.values()) {
        const item = d.items.find((i) => i.id === itemId);
        if (item) {
          if (!item.restorable) return Promise.reject(new Error("restauration impossible : l'élément n'est plus dans la Corbeille"));
          item.restorable = false;
          item.restoredAt = Date.now();
          return resolved(undefined);
        }
      }
      return Promise.reject(new Error("entrée du journal inconnue"));
    },

    search(query, limit) {
      const hits: SearchHit[] = [];
      for (const tree of Object.values(TREES)) {
        for (const [id, n] of tree.nodes) {
          if (!n.tile) continue;
          hits.push({
            id,
            name: l(n.tile.name),
            path: tree.base + l(n.tile.path),
            size: gb(n.tile.size),
            modified: resolveWhen(n.tile.last, now()),
            kind: n.tile.files > 1 ? "dir" : "file",
          });
        }
      }
      return resolved(fuzzyFilter(hits, query, (h) => `${h.name} ${h.path}`, limit));
    },

    settings() {
      return resolved({ ...settings });
    },

    updateSettings(patch) {
      settings = { ...settings, ...patch };
      writeSettings(settings);
      return resolved({ ...settings });
    },
  };
}
