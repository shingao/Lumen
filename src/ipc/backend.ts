// Contrat entre l'interface et le backend : le backend Tauri (ipc/tauri.ts, commandes Rust
// typées) dans l'appli, le backend factice (ipc/mock, données de la maquette) dans le navigateur.

import type { Locale } from "../core/format";
import type {
  AppInfo,
  CleanEvent,
  CleanMode,
  CleanPlan,
  CleanRequest,
  DormantResult,
  DuplicatesEvent,
  FileKind,
  JournalAction,
  JournalDetail,
  RestorePoints,
  Kpis,
  MoveCheck,
  MoveFinish,
  MoveProgress,
  MoveRow,
  MoveRun,
  MoveUndo,
  SavedReport,
  LargestResult,
  NodeId,
  Recommendation,
  RecommendationDetail,
  ScanEvent,
  ScanMethod,
  ScanSummary,
  ScanTarget,
  SearchHit,
  Settings,
  SeriesKey,
  ScanCacheInfo,
  DevOverview,
  DockerImages,
  GamesResult,
  ShaderCaches,
  WslDisks,
  SystemSinks,
  DailyStatus,
  DatabaseInfo,
  TimelineCompare,
  TimelineData,
  TimelineSeries,
  TreeSlice,
  Volume,
} from "./types";

export interface Backend {
  setLocale(locale: Locale): void;
  appInfo(): Promise<AppInfo>;
  volumes(): Promise<Volume[]>;
  /** Lance une analyse ; le résultat arrive par `onScanEvent`. */
  startScan(target: ScanTarget, method: ScanMethod): Promise<number>;
  cancelScan(scanId: number): Promise<void>;
  onScanEvent(listener: (event: ScanEvent) => void): () => void;
  /** Deux niveaux agrégés sous `node` (racine analysée si null). */
  tree(scanId: number, node: NodeId | null): Promise<TreeSlice>;
  /** Nœud d'un chemin complet dans l'analyse (historique : les identifiants changent d'une analyse à l'autre). */
  findNode(scanId: number, path: string): Promise<NodeId | null>;
  /** Dernière analyse d'une cible gardée pendant la session (retour instantané), ou null. */
  cachedScan(target: ScanTarget): Promise<ScanSummary | null>;
  /** Analyses enregistrées au lancement précédent, remises en mémoire (une par cible). */
  restoreScans(): Promise<ScanSummary[]>;
  /** Mémoire des analyses gardées pendant la session (Paramètres). */
  scanCacheInfo(): Promise<ScanCacheInfo>;
  kpis(target: ScanTarget, summary: ScanSummary | null): Promise<Kpis>;
  recommendations(scanId: number): Promise<Recommendation[]>;
  recommendation(id: string): Promise<RecommendationDetail>;
  /** Script PowerShell des recommandations données (simulation par défaut, garde-fous embarqués). */
  script(scanId: number, ids: readonly string[]): Promise<string>;
  /** Plan validé par safety pour la sélection (recommandations, nettoyage avancé) ; rien n'est fait. */
  planCleanup(scanId: number, request: CleanRequest, mode: CleanMode): Promise<CleanPlan>;
  /** Fichiers dormants (« À vérifier ») non utilisés depuis `months` mois. */
  dormant(scanId: number, months: number): Promise<DormantResult>;
  /** Les 100 plus gros fichiers, filtrés par type. */
  largest(scanId: number, kind: FileKind | null): Promise<LargestResult>;
  /** Recherche des doublons (lit le contenu) ; résultat par `onDuplicatesEvent`. */
  startDuplicates(scanId: number, minSizeMb: number): Promise<void>;
  cancelDuplicates(scanId: number): Promise<void>;
  onDuplicatesEvent(listener: (event: DuplicatesEvent) => void): () => void;
  /** Gouffres système, lus sans droits d'administrateur. */
  systemSinks(): Promise<SystemSinks>;
  /** Points de restauration du lecteur système (une demande UAC, lecture seule). */
  readRestorePoints(): Promise<RestorePoints>;
  /** Ouvre « Protection du système » de Windows. */
  openSystemProtection(): Promise<void>;
  /** Exécution réelle du plan affiché (son empreinte), avancement et bilan par `onCleanEvent`. */
  executePlan(planId: number, fingerprint: string, permanentAcknowledged: boolean, withoutRestorePoint: boolean): Promise<void>;
  cancelPlan(planId: number): Promise<void>;
  onCleanEvent(listener: (event: CleanEvent) => void): () => void;
  journal(): Promise<JournalAction[]>;
  journalDetail(actionId: number): Promise<JournalDetail>;
  journalScript(actionId: number): Promise<string>;
  /** « Restaurer depuis la Corbeille » d'un élément du journal. */
  restore(itemId: number): Promise<void>;
  /** Recherche dans les noms des dernières analyses (filtres `ext:`, `>1go`, `type:`, `disque:`). */
  search(query: string, limit: number): Promise<SearchHit[]>;
  /** Ouvre un résultat (`open` : appli par défaut, jamais un programme ; `reveal` : Explorateur).
   * Renvoie ce qui a été fait. */
  openItem(scanId: number, id: NodeId, action: "open" | "reveal"): Promise<"open" | "reveal">;
  /** Jeux installés, détectés en lecture seule chez leurs launchers. */
  games(): Promise<GamesResult>;
  /** Jaquette du cache local du launcher (data URL), ou null. */
  gameCover(id: string): Promise<string | null>;
  /** Ouvre le launcher pour désinstaller ou déplacer un jeu : Lumen ne le fait jamais lui-même. */
  openGame(id: string, action: "uninstall" | "move"): Promise<void>;
  /** Jeux et launchers ouverts maintenant (les caches de shaders ne se nettoient pas alors). */
  gamesRunning(): Promise<string[]>;
  shaderCaches(scanId: number | null): Promise<ShaderCaches>;
  /** Projets et caches de paquets de l'analyse affichée. */
  devOverview(scanId: number): Promise<DevOverview>;
  dockerImages(): Promise<DockerImages>;
  wslDisks(): Promise<WslDisks>;
  /** Séries de snapshots (une par volume, racine et méthode), la plus récente d'abord. */
  timelineSeries(): Promise<TimelineSeries[]>;
  timeline(series: SeriesKey): Promise<TimelineData>;
  /** Variation de l'espace utilisé sur 7 jours pour une cible (null : pas assez d'historique). */
  timelineTrend(target: ScanTarget): Promise<number | null>;
  /** Différence entre deux snapshots ; jamais de faux delta entre deux séries différentes. */
  timelineCompare(from: number, to: number): Promise<TimelineCompare>;
  /** Snapshot quotidien : appelé à intervalles réguliers tant que l'appli tourne. */
  timelineDaily(enabled: boolean): Promise<DailyStatus>;
  onTimelineChanged(listener: () => void): () => void;
  database(): Promise<DatabaseInfo>;
  /** Supprime tous les snapshots (le journal des nettoyages est gardé). */
  purgeSnapshots(): Promise<DatabaseInfo>;
  /** Vérifie le déplacement d'un dossier de l'analyse vers un autre disque (le plus libre si `drive` est null). */
  moveCheck(scanId: number, nodeId: NodeId, drive: string | null): Promise<MoveCheck>;
  /** Copie, vérification, jonction et test ; s'arrête avant toute suppression. */
  moveStart(planId: number, fingerprint: string): Promise<MoveRun>;
  /** Suppression définitive de l'original (confirmée), ou retour en arrière. */
  moveConfirm(moveId: number, deleteOriginal: boolean): Promise<MoveFinish>;
  /** Déplacements du journal et état de leurs jonctions. */
  moves(): Promise<MoveRow[]>;
  moveUndo(moveId: number): Promise<MoveUndo>;
  onMoveProgress(listener: (p: MoveProgress) => void): () => void;
  /** Enregistre un rapport dans Téléchargements, sous un nom neuf. */
  saveReport(format: "html" | "csv", stamp: string, content: string): Promise<SavedReport>;
  /** Montre le dernier rapport enregistré dans l'Explorateur. */
  revealReport(): Promise<void>;
  settings(): Promise<Settings>;
  updateSettings(patch: Partial<Settings>): Promise<Settings>;
}

/** Clé stable d'une cible d'analyse. */
export function targetKey(t: ScanTarget): string {
  return t.kind === "userProfile" ? "user" : t.letter.toUpperCase();
}
