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
  SystemSinks,
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
  search(query: string, limit: number): Promise<SearchHit[]>;
  settings(): Promise<Settings>;
  updateSettings(patch: Partial<Settings>): Promise<Settings>;
}

/** Clé stable d'une cible d'analyse. */
export function targetKey(t: ScanTarget): string {
  return t.kind === "userProfile" ? "user" : t.letter.toUpperCase();
}
