// Export du rapport : rassemble ce que l'écran Analyse affiche (KPI, treemap, recommandations)
// et l'évolution de la Timeline, construit la page HTML autonome ou le CSV (core/report.ts),
// puis le fait enregistrer par Lumen dans Téléchargements. « Anonymiser les chemins » est coché
// par défaut : un rapport est fait pour être partagé.

import { create } from "zustand";
import sansFont from "@fontsource-variable/inter/files/inter-latin-wght-normal.woff2?inline";
import monoFont from "@fontsource/jetbrains-mono/files/jetbrains-mono-latin-400-normal.woff2?inline";
import tokens from "../styles/tokens.css?raw";
import { buildCsv, buildReportHtml, type ReportInput } from "../core/report";
import { currentT } from "../i18n";
import { backend } from "../ipc/client";
import type { SavedReport, ScanTarget } from "../ipc/types";
import { useStore } from "./store";

export type ReportFormat = "html" | "csv";

export interface ExportUi {
  open: boolean;
  format: ReportFormat;
  anonymize: boolean;
  busy: boolean;
  saved: SavedReport | null;
  message: string | null;
}

const CLOSED: ExportUi = { open: false, format: "html", anonymize: true, busy: false, saved: null, message: null };

export const useExport = create<ExportUi>()(() => CLOSED);

/** Ouvre la fenêtre d'export, « Anonymiser les chemins » coché à chaque ouverture. */
export function openExport(): void {
  useExport.setState({ ...CLOSED, open: true });
}

export function closeExport(): void {
  useExport.setState({ open: false });
}

export function setExport(patch: Partial<Pick<ExportUi, "format" | "anonymize">>): void {
  useExport.setState({ ...patch, saved: null, message: null });
}

/** Horodatage du nom de fichier : `2026-10-24-1530`. */
export function stamp(at: number): string {
  const d = new Date(at);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`;
}

const sameTarget = (a: ScanTarget | null, b: ScanTarget) => a !== null && JSON.stringify(a) === JSON.stringify(b);

/** Données du rapport de l'analyse affichée (null : rien d'analysé). */
export async function reportInput(anonymize: boolean): Promise<ReportInput | null> {
  const { scan, target, appInfo, locale } = useStore.getState();
  const summary = scan.summary;
  if (!summary) return null;
  const t = currentT();
  const [slice, kpis, recommendations, series] = await Promise.all([
    backend.tree(summary.scanId, null),
    backend.kpis(target, summary).catch(() => null),
    backend.recommendations(summary.scanId).catch(() => []),
    backend.timelineSeries().catch(() => []),
  ]);
  const match = series.find((s) => sameTarget(s.target, target));
  const timeline = match ? await backend.timeline({ volumeId: match.volumeId, root: match.root, method: match.method }).catch(() => null) : null;
  return {
    locale,
    text: (key, params) => t(key as Parameters<typeof t>[0], params),
    version: appInfo?.version ?? "",
    generatedAt: Date.now(),
    targetLabel: target.kind === "userProfile" ? t("report.targetProfile") : t("report.targetVolume", { letter: target.letter }),
    rootPath: summary.rootPath,
    finishedAt: summary.finishedAt,
    method: summary.method,
    files: summary.files,
    dirs: summary.dirs,
    root: slice.root,
    kpis,
    recommendations,
    timeline: (timeline?.points ?? []).map((p) => ({ at: p.at, used: p.used })).sort((a, b) => a.at - b.at),
    anonymize,
    profile: appInfo?.profile ?? null,
  };
}

export async function runExport(): Promise<void> {
  const { format, anonymize } = useExport.getState();
  useExport.setState({ busy: true, saved: null, message: null });
  try {
    const input = await reportInput(anonymize);
    if (!input) throw new Error(currentT()("report.nothing"));
    const content = format === "html" ? buildReportHtml(input, { tokens, sans: sansFont, mono: monoFont, dark: useStore.getState().dark }) : buildCsv(input);
    const saved = await backend.saveReport(format, stamp(input.generatedAt), content);
    useExport.setState({ busy: false, saved });
  } catch (e) {
    useExport.setState({ busy: false, message: e instanceof Error ? e.message : String(e) });
  }
}
