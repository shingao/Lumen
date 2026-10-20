// Contrat entre l'interface et le backend : le backend Tauri (ipc/tauri.ts, commandes Rust
// typées) dans l'appli, le backend factice (ipc/mock, données de la maquette) dans le navigateur.

import type { Locale } from "../core/format";
import type {
  AppInfo,
  Kpis,
  NodeId,
  Recommendation,
  RecommendationDetail,
  ScanEvent,
  ScanMethod,
  ScanSummary,
  ScanTarget,
  SearchHit,
  Settings,
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
  /** Script PowerShell des recommandations données, avec ou sans -WhatIf. */
  script(ids: readonly string[], whatIf: boolean): Promise<string>;
  search(query: string, limit: number): Promise<SearchHit[]>;
  settings(): Promise<Settings>;
  updateSettings(patch: Partial<Settings>): Promise<Settings>;
}

/** Clé stable d'une cible d'analyse. */
export function targetKey(t: ScanTarget): string {
  return t.kind === "userProfile" ? "user" : t.letter.toUpperCase();
}
