// Types échangés avec le backend. Ils seront générés depuis Rust (tauri-specta) au jalon 3 ;
// d'ici là, le backend factice (ipc/mock) les implémente à l'identique.

export type Category = "media" | "code" | "packages" | "tools" | "android" | "ai" | "games" | "system";
export type Status = "safe" | "review" | "keep";
export type RecoStatus = Exclude<Status, "keep">;

/** Identifiant d'un élément du scan : le front n'envoie jamais de chemin libre. */
export type NodeId = number;

export type NodeKind = "dir" | "file" | "link" | "cloudFile" | "cloudDir";

export interface TreeNode {
  /** Identifiant dans l'analyse ; négatif pour un regroupement « autres ». */
  id: NodeId;
  name: string;
  /** Chemin complet, pour l'affichage uniquement. */
  path: string;
  kind: NodeKind;
  /** Taille allouée sur le disque (affichée par défaut). */
  size: number;
  /** Taille logique (infobulle). */
  logical: number;
  /** Regroupement des plus petits éléments : nombre d'éléments regroupés (sinon null). */
  othersCount: number | null;
  /** Surface de la tuile si elle diffère de `size` (sinon absente). */
  layoutSize?: number;
  files: number;
  /** Dernière ouverture (ms epoch), LastAccess ou LastWrite selon la fiabilité. */
  lastOpened: number;
  category: Category;
  status: Status;
  reco: string | null;
  /** Octets récupérables dans cet élément (0 si rien). */
  gain: number;
  gainStatus: RecoStatus | null;
  /** « jusqu'à » : le gain est un maximum (compactage). */
  gainUpTo: boolean;
  hasChildren: boolean;
  children: TreeNode[];
}

export interface Crumb {
  label: string;
  /** null : segment au-dessus de la racine analysée (non cliquable). */
  id: NodeId | null;
}

export interface TreeSlice {
  root: TreeNode;
  breadcrumb: Crumb[];
}

export type VolumeKind = "fixed" | "removable" | "network" | "other";

export interface Volume {
  id: string;
  letter: string;
  label: string;
  kind: VolumeKind;
  fileSystem: string;
  /** NTFS local : la lecture rapide de la MFT est possible. */
  mftCapable: boolean;
  total: number;
  free: number;
}

export type ScanTarget = { kind: "userProfile" } | { kind: "volume"; letter: string };
/** « auto » : parcours (ou MFT si l'appli est déjà élevée) ; « fast » : Scan rapide (admin). */
export type ScanMethod = "auto" | "fast";

export interface AppInfo {
  elevated: boolean;
  /** « Scan rapide (admin) » proposé : Lumen installé dans Program Files, non modifiable. */
  fastScanAvailable: boolean;
  version: string;
}

/** Information discrète sur une analyse (le parcours a pris le relais d'une MFT trop fragmentée). */
export type ScanNotice = "mftFragmented";

export interface TopProgress {
  name: string;
  allocated: number;
  files: number;
}

export interface ScanProgress {
  scanId: number;
  files: number;
  dirs: number;
  allocated: number;
  errors: number;
  elapsedMs: number;
  phase: string;
  tops: TopProgress[];
}

export interface ScanSummary {
  scanId: number;
  rootPath: string;
  method: "walk" | "mft";
  files: number;
  dirs: number;
  allocated: number;
  logical: number;
  links: number;
  cloudFiles: number;
  cloudDirs: number;
  cloudBytes: number;
  hardlinkDups: number;
  accessDenied: number;
  locked: number;
  otherErrors: number;
  elapsedMs: number;
  lastAccessReliable: boolean;
  volume: string;
  notice: ScanNotice | null;
}

export type ScanEvent =
  | { type: "progress"; progress: ScanProgress }
  | { type: "done"; summary: ScanSummary }
  | { type: "failed"; scanId: number; cancelled: boolean; message: string };

export interface Kpis {
  /** Faux tant que le moteur de règles (jalon 4) n'est pas là : pas encore de sans risque / à vérifier. */
  rulesReady: boolean;
  freeNow: number;
  freeAfter: number;
  safeBytes: number;
  safeCount: number;
  reviewBytes: number;
  reviewCount: number;
  unanalyzed: number;
  /** Variation de l'espace utilisé sur 7 jours (octets, + = le disque se remplit). */
  trend7d: number;
}

export interface Recommendation {
  id: string;
  status: RecoStatus;
  name: string;
  path: string;
  size: number;
  /** Tuiles mises en avant au survol. */
  highlight: NodeId[];
}

export interface RecoFile {
  path: string;
  size: number;
}

export interface ScriptLine {
  text: string;
  whatIfAt: number | null;
}

export interface RecommendationDetail extends Recommendation {
  explanation: string;
  meta: string;
  files: RecoFile[];
  /** Lignes du script ; `whatIfAt` : position où s'insère « -WhatIf » en simulation (null : aucune). */
  script: ScriptLine[];
  note: string;
}

export interface SearchHit {
  id: NodeId;
  name: string;
  path: string;
  size: number;
  modified: number;
  kind: "file" | "dir";
}

export type ThemePref = "system" | "light" | "dark";
export type LanguagePref = "system" | "fr" | "en";

export interface Settings {
  theme: ThemePref;
  language: LanguagePref;
  excludedFolders: string[];
  dormantMonths: 3 | 6 | 12 | 24;
  duplicateMinMb: number;
  permanentDelete: boolean;
  dailySnapshot: boolean;
  trayIcon: boolean;
  lowSpaceAlert: boolean;
  lowSpaceThreshold: number;
  explorerMenu: boolean;
  logsFolder: string;
}

// --- Nettoyage (jalon 5) -------------------------------------------------------------------

/** Corbeille (par défaut) ou suppression définitive (option des réglages, double confirmation). */
export type CleanMode = "trash" | "permanent";

export interface PlanItem {
  path: string;
  size: number;
  directory: boolean;
  rule: string;
  ruleName: string;
}

export interface PlanCommand {
  line: string;
  explanation: string;
  prerequisite: boolean;
  needsElevation: boolean;
  size: number;
}

/** Plan validé par safety, à confirmer : rien n'est fait tant qu'il ne l'est pas. */
export interface CleanPlan {
  planId: number;
  /** Empreinte du plan affiché : l'exécution n'accepte que celle-ci. */
  fingerprint: string;
  mode: CleanMode;
  total: number;
  /** Éléments supprimés + commandes officielles qui sont des actions. */
  count: number;
  items: PlanItem[];
  moreItems: number;
  commands: PlanCommand[];
  volumes: { volume: string; free: number | null }[];
  refused: { path: string; reason: string }[];
  skippedLocked: number;
}

export interface CleanProgress {
  planId: number;
  phase: "command" | "item" | "verify";
  index: number;
  total: number;
  label: string;
  verified: number;
  freed: number;
}

export type ItemOutcome = "done" | "partial" | "skipped" | "refused" | "failed" | "cancelled";
export type CommandOutcome = "succeeded" | "failed" | "timedOut" | "notFound" | "needsElevation" | "cancelled" | "error";

export interface CleanReport {
  planId: number;
  actionId: number | null;
  status: "ok" | "partial" | "failed" | "cancelled";
  mode: CleanMode;
  /** Somme estimée de ce qui est parti. */
  freed: number;
  /** Gain réel : espace libre du disque avant et après (null si illisible). */
  measured: number | null;
  done: number;
  partial: number;
  skipped: number;
  refused: number;
  failed: number;
  cancelled: number;
  items: {
    path: string;
    planned: number;
    freed: number;
    outcome: ItemOutcome;
    reason: string | null;
    issues: { path: string; reason: string }[];
    moreIssues: number;
  }[];
  commands: { line: string; status: CommandOutcome; detail: string | null }[];
  journalError: string | null;
}

export type CleanEvent =
  | { type: "progress"; progress: CleanProgress }
  | { type: "done"; report: CleanReport }
  | { type: "failed"; planId: number; message: string };

export interface JournalAction {
  id: number;
  startedAt: number;
  finishedAt: number;
  title: string;
  method: CleanMode;
  status: CleanReport["status"];
  bytesPlanned: number;
  bytesFreed: number;
  bytesMeasured: number | null;
  itemsDone: number;
  itemsPartial: number;
  itemsSkipped: number;
  itemsRefused: number;
  itemsFailed: number;
  itemsCancelled: number;
}

export interface JournalItem {
  id: number;
  path: string;
  ruleId: string | null;
  directory: boolean;
  size: number;
  method: CleanMode;
  result: "done" | "failed" | "left" | "skipped" | "refused" | "cancelled";
  reason: string | null;
  restoredAt: number | null;
  /** Encore dans la Corbeille, emplacement d'origine libre : « Restaurer » possible. */
  restorable: boolean;
}

export interface JournalCommand {
  role: "prerequisite" | "action";
  program: string | null;
  args: string[];
  status: CommandOutcome;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
}

export interface JournalDetail {
  action: JournalAction;
  items: JournalItem[];
  commands: JournalCommand[];
}
