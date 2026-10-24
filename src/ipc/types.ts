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
  /** Dossier utilisateur : il devient `%USERPROFILE%` dans un rapport anonymisé. */
  profile: string | null;
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
  /** Fin de l'analyse (ms) : « analysé il y a… ». */
  finishedAt: number;
  /** Relue depuis la base au démarrage (analyse d'un lancement précédent). */
  restored: boolean;
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
  /** Analyse d'où vient le résultat (une par disque) et nœud dans celle-ci. */
  scanId: number;
  id: NodeId;
  name: string;
  path: string;
  size: number;
  modified: number;
  kind: "file" | "dir";
  /** Lettre du disque (`C:`). */
  drive: string;
  /** `userProfile` ou la lettre du disque analysé. */
  target: string;
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
  /** Onglet Gouffres système (commandes administrateur) : désactivé tant qu'il n'est pas vérifié en VM. */
  experimentalSystem: boolean;
  /** Compactage des disques WSL / Docker (jalon 8), désactivé tant qu'il n'est pas vérifié en VM. */
  experimentalWsl: boolean;
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
  /** Prérequis obligatoire : s'il n'est pas rempli, rien d'autre n'est fait. */
  required: boolean;
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
  /** Commandes « administrateur » lancées par Lumen : une demande UAC chacune. */
  elevated: boolean;
  /** Actions système sans point de restauration préalable : « Continuer sans point de restauration » exigé. */
  withoutRestorePoint: boolean;
  /** Refusé au moment de l'action si un jeu ou un launcher tourne (caches de shaders). */
  processGuard: boolean;
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
export type CommandOutcome = "succeeded" | "failed" | "timedOut" | "notFound" | "needsElevation" | "declined" | "cancelled" | "error";

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
  /** `satisfied` : a fait son travail (un point de restauration déjà récent compte). */
  commands: { line: string; status: CommandOutcome; satisfied: boolean; detail: string | null; log: string | null }[];
  /** Arrêt avant les actions : un prérequis obligatoire n'a pas été rempli. */
  aborted: string | null;
  /** Le point de restauration n'a pas pu être créé (protection du système désactivée…). */
  restorePointFailed: boolean;
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
  kind: CleanKind;
  aborted: string | null;
  /** Lancé sans point de restauration, après « Continuer sans point de restauration ». */
  withoutRestorePoint: boolean;
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
  /** Journal de DISM rattaché, et sa fin. */
  logPath: string | null;
  logTail: string | null;
}

export interface JournalDetail {
  action: JournalAction;
  items: JournalItem[];
  commands: JournalCommand[];
}

// --- Nettoyage avancé (jalon 6) ------------------------------------------------------------

export type CleanKind = "recos" | "dormant" | "largest" | "duplicates" | "system" | "shaders" | "artifacts" | "docker" | "wsl";

export type FileKind = "video" | "image" | "audio" | "archive" | "disk" | "installer" | "document" | "other";
export const FILE_KINDS: readonly FileKind[] = ["video", "image", "audio", "archive", "disk", "installer", "document", "other"];

/** Un fichier proposé : toujours « À vérifier ». */
export interface FileRow {
  id: number;
  name: string;
  /** Chemin affiché (`~\…`). */
  path: string;
  size: number;
  logical: number;
  modified: number;
  accessed: number;
  kind: FileKind;
  /** Dans un dossier d'appli : à vérifier avec soin. */
  app: boolean;
}

export interface DormantResult {
  months: number;
  /** Date utilisée : dernier accès (si Windows le tient à jour) ou dernière modification. */
  basis: "lastAccess" | "lastWrite";
  /** Valeur brute de `NtfsDisableLastAccessUpdate` (`fsutil behavior query disablelastaccess`). */
  lastAccessRaw: number | null;
  total: number;
  count: number;
  folders: { path: string; size: number; count: number; files: FileRow[] }[];
}

export interface LargestResult {
  files: FileRow[];
  kinds: [FileKind, number][];
}

export interface DuplicateGroup {
  id: number;
  size: number;
  /** Ce que libère la suppression de tous les exemplaires sauf un. */
  wasted: number;
  members: FileRow[];
  /** Exemplaire présélectionné pour être gardé (le plus ancien). */
  keep: number;
}

export interface DuplicateStats {
  candidates: number;
  read: number;
  cached: number;
  skippedCloud: number;
  skippedNotFrozen: number;
  errors: number;
  hardLinks: number;
}

export interface DuplicatesResult {
  scanId: number;
  groups: DuplicateGroup[];
  moreGroups: number;
  wasted: number;
  stats: DuplicateStats;
}

export interface DuplicatesProgress {
  scanId: number;
  phase: "edges" | "full";
  done: number;
  total: number;
}

export type DuplicatesEvent =
  | { type: "progress"; progress: DuplicatesProgress }
  | { type: "done"; result: DuplicatesResult }
  | { type: "failed"; scanId: number; cancelled: boolean; message: string };

export type SystemAction = "components" | "hibernateOff" | "hibernateReduced" | "oldestRestorePoint" | "windowsOld";

export interface SystemSinks {
  /** Faits lus sur ce PC (faux hors de Windows). */
  available: boolean;
  systemDrive: string;
  hiberfil: number | null;
  pagefile: number | null;
  swapfile: number | null;
  hibernateEnabled: boolean | null;
  fastStartup: boolean | null;
  windowsOld: boolean;
  restorePointFrequencyMin: number;
  lastAccessRaw: number | null;
  lastAccessUpdated: boolean;
  free: number | null;
  /** Protection du système activée sur le lecteur système (null : inconnue). */
  restoreProtection: boolean | null;
}

export interface RestorePoint {
  id: string;
  created: number;
  description: string | null;
}

/** Points de restauration du lecteur système, lus en administrateur (lecture seule). */
export interface RestorePoints {
  drive: string;
  count: number;
  points: RestorePoint[];
  /** Le point qui serait supprimé : le plus ancien. */
  oldest: RestorePoint | null;
  deletable: boolean;
  /** Pourquoi il est gardé (moins de 2 points, moins de 7 jours ou de cette session). */
  refusal: string | null;
}

/** Ce que l'interface demande à safety : jamais un chemin, seulement des identifiants. */
export type CleanRequest =
  | { kind: "recos"; ids: string[] }
  | { kind: "dormant"; ids: number[] }
  | { kind: "largest"; ids: number[] }
  | { kind: "duplicates"; ids: number[] }
  | { kind: "system"; actions: SystemAction[]; restorePoint: boolean }
  | { kind: "shaders" }
  | { kind: "artifacts"; ids: number[] }
  | { kind: "dockerPrune" }
  | { kind: "wslCompact"; ids: number[] };

// --- Timeline (jalon 7) ---------------------------------------------------------------------

/** Série de snapshots comparables : un volume (par son GUID), une racine, une méthode. */
export interface SeriesKey {
  volumeId: number;
  /** Racine relative au volume (« \ » pour le volume entier). */
  root: string;
  method: "walk" | "mft";
}

export interface TimelineSeries extends SeriesKey {
  label: string;
  fileSystem: string;
  /** Lettre actuelle ; null si le volume n'est pas monté. */
  letter: string | null;
  /** Lettre au dernier snapshot (elle peut avoir changé depuis). */
  lastLetter: string;
  profile: boolean;
  target: ScanTarget | null;
  count: number;
  first: number;
  last: number;
  /** Snapshot le plus récent de la série (comparaison avec une autre série). */
  lastId: number;
}

export interface TimelinePoint {
  id: number;
  at: number;
  used: number;
  source: "scan" | "daily";
}

/** Nettoyage de Lumen, marqué sur la courbe. */
export interface TimelineMarker {
  actionId: number;
  at: number;
  bytes: number;
  title: string;
  kind: CleanKind;
}

export interface TimelineProjection {
  /** `tooFew` : moins de 5 points sur 30 jours ; `flat` : pente nulle ou négative. */
  status: "tooFew" | "flat" | "growing";
  points: number;
  bytesPerDay: number | null;
  daysToFull: number | null;
}

export interface TimelineData {
  points: TimelinePoint[];
  markers: TimelineMarker[];
  projection: TimelineProjection;
  free: number | null;
  total: number | null;
  trend7d: number | null;
  /** Chemin complet actuel de la racine (pour ouvrir l'Explorer), null si le volume est absent. */
  base: string | null;
  target: ScanTarget | null;
}

export interface FolderChange {
  /** Relatif à la racine de la série. */
  path: string;
  before: number | null;
  after: number | null;
  delta: number;
}

export interface TimelineDiff {
  grown: FolderChange[];
  shrunk: FolderChange[];
  added: FolderChange[];
  removed: FolderChange[];
  total: number;
}

export interface SnapshotRef {
  id: number;
  at: number;
  used: number;
  root: string;
  method: "walk" | "mft";
  letter: string;
}

export interface TimelineCompare {
  comparable: boolean;
  reason: "volume" | "root" | "method" | null;
  from: SnapshotRef;
  to: SnapshotRef;
  diff: TimelineDiff | null;
  /** Dossiers plus petits non gardés dans les snapshots. */
  minBytes: number;
  base: string | null;
  target: ScanTarget | null;
}

export interface DailyStatus {
  state: "off" | "idle" | "running" | "waiting";
  label: string | null;
  lastAt: number | null;
  lastOutcome: "done" | "failed" | "cancelled" | null;
  /** Séries analysées par la MFT, sans snapshot quotidien faute de droits d'administrateur. */
  needsAdmin: number;
}

export interface DatabaseInfo {
  bytes: number;
  snapshots: number;
  series: number;
  /** Analyses enregistrées (une par cible) et leur taille compressée. */
  savedScans: number;
  savedBytes: number;
}

// --- Jeux et Dev (jalon 8) ------------------------------------------------------------------

export type LauncherId = "steam" | "epic" | "gog" | "battlenet" | "ea" | "ubisoft" | "riot" | "curseforge" | "minecraft" | "xbox";

export interface Game {
  /** `launcher:clé`, seul identifiant renvoyé au backend. */
  id: string;
  launcher: LauncherId;
  name: string;
  installDir: string | null;
  size: number | null;
  /** `launcher` : taille annoncée ; `measured` : mesurée sur le disque. */
  sizeSource: "launcher" | "measured" | "unknown";
  drive: string | null;
  /** Dernière partie (ms), seulement si le launcher la connaît : jamais une date inventée. */
  lastPlayed: number | null;
  status: "keep" | "review";
  cover: boolean;
  /** Ce que Lumen peut ouvrir : adresse Steam ou appli du Store, le launcher, ou rien. */
  open: "uri" | "app" | "none";
  /** Le launcher sait-il déplacer un jeu ? (Riot Client, CurseForge : non.) */
  canMove: boolean;
  /** Données et sauvegardes hors du dossier du jeu, protégées (jamais proposées). */
  data: { size: number; paths: string[] } | null;
}

export interface GamesResult {
  games: Game[];
  /** Fichiers ou clés de launchers ignorés (absents, illisibles, mal formés) : comptés. */
  ignored: { launcher: LauncherId; file: string; reason: "missing" | "unreadable" | "tooLarge" | "malformed" | "incomplete" }[];
  launchers: LauncherId[];
  steamShaderCaches: { path: string; size: number }[];
}

export interface ShaderCaches {
  /** L'analyse affichée couvre-t-elle les caches de shaders ? */
  analysed: boolean;
  caches: { path: string; size: number; files: number }[];
  total: number;
}

export interface ProjectArtifact {
  id: NodeId;
  path: string;
  size: number;
  status: "safe" | "review";
}

export interface DevProject {
  id: NodeId;
  name: string;
  path: string;
  language: string | null;
  lastActivity: number | null;
  activitySource: "git" | "files";
  git: boolean;
  lockfile: boolean;
  sourceSize: number;
  gitSize: number;
  artifactsSize: number;
  artifacts: ProjectArtifact[];
}

export interface DevCache {
  id: string;
  status: RecoStatus;
  name: string;
  path: string;
  size: number;
  /** Nettoyé par la commande officielle de l'outil (sinon : contenu à la Corbeille). */
  command: boolean;
}

export interface DevOverview {
  projects: DevProject[];
  caches: DevCache[];
}

export interface DockerImages {
  running: boolean;
  reason: "notInstalled" | "notRunning" | null;
  images: { id: string; repository: string; tag: string; size: number; created: string }[];
  total: number;
}

export interface WslDisks {
  disks: { id: number; name: string; kind: "wsl" | "docker"; path: string; size: number }[];
  /** Module Hyper-V présent : Optimize-VHD ; sinon diskpart en lecture seule. */
  hyperv: boolean;
}

/** Analyses gardées en mémoire pendant la session (une par disque), bornées. */
export interface ScanCacheInfo {
  /** Du plus récemment utilisé au moins récent ; `target` : `userProfile` ou la lettre du disque. */
  entries: { scanId: number; target: string; bytes: number; lastUsed: number }[];
  total: number;
  limit: number;
}

// Déplacement intelligent (jalon 9) : le front n'envoie qu'un id d'analyse, un id de nœud et une lettre.

export interface MoveRefusal {
  code: string;
  message: string;
  /** Programmes qui tiennent des fichiers du dossier. */
  apps: string[];
}

export interface MoveDrive {
  letter: string;
  label: string;
  free: number;
}

export interface MoveCheck {
  /** Présent seulement si le déplacement est possible. */
  planId: number | null;
  fingerprint: string | null;
  source: string;
  target: string | null;
  drive: string | null;
  drives: MoveDrive[];
  bytes: number;
  files: number;
  /** Taille plus 10 %. */
  needed: number;
  free: number;
  /** Dossier d'appli dans AppData : « À vérifier ». */
  review: boolean;
  refusals: MoveRefusal[];
}

export type MoveStep = "recheck" | "copy" | "verify" | "setAside" | "junction" | "test" | "delete";

export interface MoveProgress {
  moveId: number;
  step: MoveStep;
  done: number;
  total: number;
}

export interface MoveRun {
  moveId: number;
  /** ready : jonction testée, en attente de la confirmation de suppression de l'original. */
  outcome: "ready" | "refused" | "rolledBack" | "failed";
  step: MoveStep | null;
  message: string | null;
  refusals: MoveRefusal[];
  source: string;
  target: string;
  bytes: number;
  files: number;
}

export interface MoveFinish {
  outcome: "done" | "rolledBack" | "deleteFailed" | "rollbackFailed";
  message: string | null;
  freed: number;
}

export type MoveHealth = "ok" | "targetVolumeAbsent" | "targetMissing" | "junctionMissing";

export interface MoveRow {
  id: number;
  startedAt: number;
  finishedAt: number | null;
  undoneAt: number | null;
  source: string;
  target: string;
  bytes: number;
  files: number;
  state: "running" | "done" | "rolledBack" | "failed" | "undone";
  step: string | null;
  message: string | null;
  /** Déplacements faits seulement. */
  health: MoveHealth | null;
}

export interface MoveUndo {
  outcome: "done" | "refused" | "failed";
  message: string | null;
}

export interface SavedReport {
  path: string;
  name: string;
}
