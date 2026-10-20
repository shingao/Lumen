// Contrat entre l'interface et le backend. Le backend factice (ipc/mock) l'implémente au jalon 1 ;
// le backend Tauri (commandes Rust typées) le remplacera sans changer les écrans.

import type { Locale } from "../core/format";
import type {
  Kpis,
  NodeId,
  Recommendation,
  RecommendationDetail,
  SearchHit,
  Settings,
  TreeSlice,
  Volume,
} from "./types";

export interface Backend {
  setLocale(locale: Locale): void;
  volumes(): Promise<Volume[]>;
  kpis(volume: string): Promise<Kpis>;
  /** Deux niveaux sous `node` (racine analysée si null). */
  tree(volume: string, node: NodeId | null): Promise<TreeSlice>;
  recommendations(volume: string): Promise<Recommendation[]>;
  recommendation(id: string): Promise<RecommendationDetail>;
  /** Script PowerShell des recommandations données, avec ou sans -WhatIf. */
  script(ids: readonly string[], whatIf: boolean): Promise<string>;
  search(query: string, limit: number): Promise<SearchHit[]>;
  settings(): Promise<Settings>;
  updateSettings(patch: Partial<Settings>): Promise<Settings>;
}
