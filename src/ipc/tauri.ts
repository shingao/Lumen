// Backend réel : commandes Rust typées (src-tauri/src/scanning.rs) et événements de l'analyse.
// Avec ipc/client.ts, seul fichier qui parle à Tauri.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatBytes, formatCount, formatRelative, type Locale } from "../core/format";
import { currentT } from "../i18n";
import type { Backend } from "./backend";
import { DEFAULT_SETTINGS } from "./mock";
import type {
  AppInfo,
  Category,
  CleanEvent,
  CleanPlan,
  CleanProgress,
  CleanReport,
  DormantResult,
  DuplicatesProgress,
  DuplicatesResult,
  LargestResult,
  RestorePoints,
  SystemSinks,
  JournalAction,
  JournalDetail,
  Kpis,
  NodeKind,
  Recommendation,
  RecommendationDetail,
  RecoStatus,
  ScriptLine,
  Status,
  ScanEvent,
  ScanProgress,
  ScanSummary,
  Settings,
  TreeNode,
  TreeSlice,
  Volume,
  VolumeKind,
} from "./types";

interface TreeNodeDto {
  id: number;
  name: string;
  path: string;
  kind: NodeKind;
  size: number;
  logical: number;
  files: number;
  lastOpened: number;
  others: number | null;
  hasChildren: boolean;
  status: Status;
  category: Category;
  reco: string | null;
  gain: number;
  gainStatus: RecoStatus | null;
  gainUpTo: boolean;
  children: TreeNodeDto[];
}

interface Text {
  fr: string;
  en: string;
}

interface RecoDto {
  id: string;
  status: RecoStatus;
  category: Category;
  name: Text;
  path: string;
  size: number;
  logical: number;
  files: number;
  dirs: number;
  lastUsed: number;
  upTo: boolean;
  action: "delete" | "command";
  projects: number;
  lastActivity: number;
  /** Éléments couverts. */
  covered: number;
  highlight: number[];
}

interface RecoDetailDto extends RecoDto {
  explanation: Text;
  regeneratedBy: Text | null;
  fileList: { path: string; size: number }[];
  commands: { line: string; explanation: string }[];
  projectList: { path: string; lastActivity: number; source: "git" | "files" }[];
}

interface RuleKpisDto {
  safeBytes: number;
  safeCount: number;
  reviewBytes: number;
  reviewCount: number;
}

interface VolumeDto {
  letter: string;
  label: string;
  fileSystem: string;
  kind: VolumeKind;
  total: number;
  free: number;
  mftCapable: boolean;
}

/** Plan tel que le renvoie Rust : nom de règle en (fr, en). */
interface PlanDto extends Omit<CleanPlan, "items"> {
  items: (Omit<CleanPlan["items"][number], "ruleName"> & { ruleName: [string, string] })[];
}

const SETTINGS_KEY = "lumen-settings";

function toNode(dto: TreeNodeDto): TreeNode {
  // Statut, catégorie et gains viennent du moteur de règles (rules.toml), jamais du nom.
  return {
    id: dto.id,
    name: dto.name,
    path: dto.path,
    kind: dto.kind,
    size: dto.size,
    logical: dto.logical,
    othersCount: dto.others,
    files: dto.files,
    lastOpened: dto.lastOpened,
    category: dto.category,
    status: dto.status,
    reco: dto.reco,
    gain: dto.gain,
    gainStatus: dto.gainStatus,
    gainUpTo: dto.gainUpTo,
    hasChildren: dto.hasChildren,
    children: dto.children.map(toNode),
  };
}

const pick = (text: Text, locale: Locale) => (locale === "en" ? text.en : text.fr);

function toReco(dto: RecoDto, locale: Locale): Recommendation {
  return { id: dto.id, status: dto.status, name: pick(dto.name, locale), path: dto.path, size: dto.size, highlight: dto.highlight };
}

/** « 412 380 fichiers · 3 dossiers · dernière utilisation il y a 2 jours ». */
function metaOf(dto: RecoDto): string {
  const t = currentT();
  const n = (v: number) => formatCount(v, t.locale);
  const parts = [t.plural("reco.meta.files", dto.files, { n: n(dto.files) })];
  if (dto.dirs > 1) parts.push(t.plural("reco.meta.folders", dto.dirs, { n: n(dto.dirs) }));
  if (dto.projects > 0) {
    parts.push(t.plural("reco.meta.projects", dto.projects, { n: n(dto.projects) }));
    if (dto.lastActivity > 0) parts.push(t("reco.meta.lastActivity", { when: formatRelative(dto.lastActivity, t.locale) }));
  } else if (dto.lastUsed > 0) {
    parts.push(t("reco.meta.lastUsed", { when: formatRelative(dto.lastUsed, t.locale) }));
  }
  if (dto.upTo) parts.push(t("reco.meta.upTo"));
  return parts.join(" · ");
}

/** Aperçu des commandes ; le script complet, simulé par safety, arrive au jalon 5. */
function scriptOf(dto: RecoDetailDto): ScriptLine[] {
  const t = currentT();
  const line = (text: string): ScriptLine => ({ text, whatIfAt: null });
  const out = [line(t("detail.script.header", { name: pick(dto.name, t.locale), size: formatBytes(dto.size, t.locale) }))];
  for (const c of dto.commands) {
    out.push(line(`# ${c.explanation}`), line(c.line));
  }
  if (dto.action === "delete") out.push(line(t.plural("detail.script.delete", dto.covered, { n: formatCount(dto.covered, t.locale) })));
  return out;
}

function readSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...DEFAULT_SETTINGS, ...(JSON.parse(raw) as Partial<Settings>) };
  } catch {
    // Réglages par défaut.
  }
  return { ...DEFAULT_SETTINGS };
}

export function createTauriBackend(): Backend {
  let settings = readSettings();
  let volumes: Volume[] = [];
  let locale: Locale = "fr";
  // Analyse affichée : les détails de recommandation s'y rapportent.
  let lastScanId = 0;

  const loadVolumes = async (): Promise<Volume[]> => {
    const list = await invoke<VolumeDto[]>("volumes_list");
    volumes = list.map((v) => ({
      id: v.letter,
      letter: v.letter,
      label: v.label,
      kind: v.kind,
      fileSystem: v.fileSystem,
      mftCapable: v.mftCapable,
      total: v.total,
      free: v.free,
    }));
    return volumes;
  };

  return {
    setLocale(next: Locale) {
      locale = next;
    },

    appInfo: () => invoke<AppInfo>("app_info"),

    volumes: loadVolumes,

    startScan: (target, method) => invoke<number>("scan_start", { target, method }),

    cancelScan: (scanId) => invoke<void>("scan_cancel", { scanId }),

    onScanEvent(listener) {
      const offs = [
        listen<ScanProgress>("scan:progress", (e) => listener({ type: "progress", progress: e.payload })),
        listen<ScanSummary>("scan:done", (e) => listener({ type: "done", summary: e.payload })),
        listen<{ scanId: number; cancelled: boolean; message: string }>("scan:failed", (e) =>
          listener({ type: "failed", ...e.payload } satisfies ScanEvent),
        ),
      ];
      return () => offs.forEach((p) => void p.then((off) => off()));
    },

    async tree(scanId, node) {
      lastScanId = scanId;
      const slice = await invoke<{ root: TreeNodeDto; breadcrumb: { label: string; id: number | null }[] }>("scan_tree", {
        scanId,
        node,
      });
      const result: TreeSlice = { root: toNode(slice.root), breadcrumb: slice.breadcrumb };
      return result;
    },

    findNode: (scanId, path) => invoke<number | null>("scan_find", { scanId, path }),
    async kpis(target, summary) {
      const list = volumes.length > 0 ? volumes : await loadVolumes();
      const letter = target.kind === "volume" ? target.letter : (summary?.volume ?? "C:");
      const vol = list.find((v) => v.letter.toUpperCase() === letter.toUpperCase());
      const free = vol?.free ?? 0;
      const used = vol ? vol.total - vol.free : 0;
      const rules = summary ? await invoke<RuleKpisDto | null>("rule_kpis", { scanId: summary.scanId }) : null;
      const k: Kpis = {
        rulesReady: rules !== null,
        freeNow: free,
        // Après nettoyage des éléments sans risque (cochés par défaut).
        freeAfter: free + (rules?.safeBytes ?? 0),
        safeBytes: rules?.safeBytes ?? 0,
        safeCount: rules?.safeCount ?? 0,
        reviewBytes: rules?.reviewBytes ?? 0,
        reviewCount: rules?.reviewCount ?? 0,
        // Ce que l'analyse n'a pas couvert sur ce disque : système, applis, autres dossiers.
        unanalyzed: summary ? Math.max(0, used - summary.allocated) : 0,
        trend7d: 0,
      };
      return k;
    },

    async recommendations(scanId) {
      lastScanId = scanId;
      const list = await invoke<RecoDto[]>("recommendations", { scanId });
      return list.map((r) => toReco(r, locale));
    },

    async recommendation(id) {
      const dto = await invoke<RecoDetailDto>("recommendation_detail", { scanId: lastScanId, id });
      const t = currentT();
      const detail: RecommendationDetail = {
        ...toReco(dto, locale),
        explanation: pick(dto.explanation, locale),
        meta: metaOf(dto),
        files: dto.fileList,
        script: scriptOf(dto),
        note: t(dto.status === "safe" ? "reco.note.safe" : "reco.note.review"),
      };
      return detail;
    },

    // Script généré par safety pour le plan des recommandations données (simulation par défaut).
    async script(scanId, ids) {
      const plan = await invoke<PlanDto>("plan_create", { scanId, recos: ids, mode: "trash" });
      return invoke<string>("plan_script", { planId: plan.planId });
    },

    async planCleanup(scanId, request, mode) {
      const dto =
        request.kind === "recos"
          ? await invoke<PlanDto>("plan_create", { scanId, recos: request.ids, mode })
          : await invoke<PlanDto>("advanced_plan", { scanId, request, mode });
      const plan: CleanPlan = { ...dto, items: dto.items.map((i) => ({ ...i, ruleName: locale === "en" ? i.ruleName[1] : i.ruleName[0] })) };
      return plan;
    },

    executePlan: (planId, fingerprint, permanentAcknowledged, withoutRestorePoint) =>
      invoke("plan_execute", { planId, fingerprint, permanentAcknowledged, withoutRestorePoint }),

    cancelPlan: (planId) => invoke("plan_cancel", { planId }),

    onCleanEvent(listener) {
      const offs = [
        listen<CleanProgress>("clean:progress", (e) => listener({ type: "progress", progress: e.payload })),
        listen<CleanReport>("clean:done", (e) => listener({ type: "done", report: e.payload })),
        listen<{ planId: number; message: string }>("clean:failed", (e) => listener({ type: "failed", ...e.payload } satisfies CleanEvent)),
      ];
      return () => offs.forEach((p) => void p.then((off) => off()));
    },

    dormant: (scanId, months) => invoke<DormantResult>("dormant_find", { scanId, months }),
    largest: (scanId, kind) => invoke<LargestResult>("largest_files", { scanId, kind }),
    startDuplicates: (scanId, minSizeMb) => invoke("duplicates_start", { scanId, minSizeMb }),
    cancelDuplicates: (scanId) => invoke("duplicates_cancel", { scanId }),
    onDuplicatesEvent(listener) {
      const offs = [
        listen<DuplicatesProgress>("dup:progress", (e) => listener({ type: "progress", progress: e.payload })),
        listen<DuplicatesResult>("dup:done", (e) => listener({ type: "done", result: e.payload })),
        listen<{ scanId: number; cancelled: boolean; message: string }>("dup:failed", (e) => listener({ type: "failed", ...e.payload })),
      ];
      return () => offs.forEach((p) => void p.then((off) => off()));
    },
    systemSinks: () => invoke<SystemSinks>("system_sinks"),
    readRestorePoints: () => invoke<RestorePoints>("restore_points_read"),
    openSystemProtection: () => invoke("open_system_protection"),
    journal: () => invoke<JournalAction[]>("journal_list"),
    journalDetail: (actionId) => invoke<JournalDetail>("journal_detail", { actionId }),
    journalScript: (actionId) => invoke<string>("journal_script", { actionId }),
    restore: (itemId) => invoke("journal_restore", { itemId }),

    // Recherche dans l'index du scan : jalon 9.
    search: () => Promise.resolve([]),

    settings: () => Promise.resolve({ ...settings }),

    updateSettings(patch) {
      settings = { ...settings, ...patch };
      try {
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
      } catch {
        // Non persistant : sans conséquence.
      }
      return Promise.resolve({ ...settings });
    },
  };
}
