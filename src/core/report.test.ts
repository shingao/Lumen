import { describe, expect, it } from "vitest";
import { buildCsv, buildReportHtml, escapeHtml, type ReportInput } from "./report";
import type { TreeNode } from "../ipc/types";

function node(path: string, size: number, patch: Partial<TreeNode> = {}, children: TreeNode[] = []): TreeNode {
  return {
    id: Math.floor(Math.random() * 1e6),
    name: path.split("\\").pop() ?? path,
    path,
    kind: "dir",
    size,
    logical: size,
    othersCount: null,
    files: 1,
    lastOpened: Date.UTC(2026, 0, 2),
    category: "media",
    status: "keep",
    reco: null,
    gain: 0,
    gainStatus: null,
    gainUpTo: false,
    hasChildren: children.length > 0,
    children,
    ...patch,
  };
}

const P = "C:\\Users\\Arthur";
const root = node(P, 10_000, {}, [
  node(`${P}\\Documents`, 6000, {}, [
    node(`${P}\\Documents\\Impôts Arthur 2025`, 4000),
    node(`${P}\\Documents\\=HYPERLINK("x")`, 1000, { kind: "file" }),
  ]),
  node(`${P}\\AppData`, 3000, { category: "packages", status: "safe" }, [node(`${P}\\AppData\\Local\\npm-cache`, 3000, { category: "packages", status: "safe", gain: 3000 })]),
  node(`${P}\\<script>alert(1)</script>`, 1000, { category: "tools" }),
]);

function input(patch: Partial<ReportInput> = {}): ReportInput {
  return {
    locale: "fr",
    text: (k, p) => (p ? `${k}${JSON.stringify(p)}` : k),
    version: "0.9.0",
    generatedAt: Date.UTC(2026, 9, 24, 10),
    targetLabel: "Dossier utilisateur",
    rootPath: P,
    finishedAt: Date.UTC(2026, 9, 24, 9),
    method: "walk",
    files: 12,
    dirs: 3,
    root,
    kpis: { rulesReady: true, freeNow: 1e9, freeAfter: 2e9, safeBytes: 1e9, safeCount: 2, reviewBytes: 5e8, reviewCount: 1, unanalyzed: 0, trend7d: 0 },
    recommendations: [{ id: "npm", status: "safe", name: "Cache npm", path: "~\\AppData\\Local\\npm-cache", size: 3000, highlight: [] }],
    timeline: [
      { at: Date.UTC(2026, 9, 1), used: 5e10 },
      { at: Date.UTC(2026, 9, 20), used: 6e10 },
    ],
    anonymize: true,
    profile: P,
    ...patch,
  };
}

const assets = { tokens: ":root{--bg-app:#FAFAF9}", sans: "data:font/woff2;base64,AAAA", mono: "data:font/woff2;base64,BBBB", dark: false };

describe("rapport HTML", () => {
  it("autonome : aucune ressource externe, aucun script", () => {
    const html = buildReportHtml(input(), assets);
    expect(html).not.toMatch(/https?:\/\//);
    expect(html).not.toMatch(/<script|<link|<iframe|<img/i);
    expect(html).not.toMatch(/\bsrc\s*=\s*["'](?!data:)/i);
    expect(html).toContain("url(data:font/woff2;base64,AAAA)");
    expect(html).toContain("default-src 'none'");
    expect(html).toContain('data-theme="light"');
    // Treemap, KPI, recommandations et Timeline sont là.
    expect(html).toContain('class="treemap"');
    expect(html).toContain("kpi.freeAfter");
    expect(html).toContain("Cache npm");
    expect(html).toContain("<polyline");
  });

  it("anonymise par défaut : ni le nom du compte ni les noms personnels", () => {
    const html = buildReportHtml(input(), assets);
    expect(html).not.toContain("Arthur");
    expect(html).not.toContain("Impôts");
    expect(html).toContain("%USERPROFILE%");
    const clear = buildReportHtml(input({ anonymize: false }), assets);
    expect(clear).toContain("Impôts Arthur 2025");
  });

  it("échappe tout ce qui vient des noms", () => {
    const html = buildReportHtml(input({ anonymize: false }), assets);
    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain(escapeHtml("<script>alert(1)</script>"));
  });

  it("sans historique ni recommandations", () => {
    const html = buildReportHtml(input({ timeline: [], recommendations: [], kpis: null }), assets);
    expect(html).toContain("report.noHistory");
    expect(html).toContain("report.noRecos");
  });
});

describe("rapport CSV", () => {
  it("une ligne par élément, BOM, séparateur selon la langue", () => {
    const csv = buildCsv(input());
    expect(csv.startsWith("\uFEFF")).toBe(true);
    const lines = csv.trim().split("\r\n");
    expect(lines).toHaveLength(1 + 6);
    expect(lines[0]).toContain(";");
    expect(buildCsv(input({ locale: "en" })).split("\r\n")[0]).toContain(",");
    expect(csv).not.toContain("Arthur");
    expect(csv).toContain("%USERPROFILE%\\AppData\\Local\\npm-cache;npm-cache;dir;3000;3000;packages;safe;3000;2026-01-02");
  });

  it("jamais une formule de tableur", () => {
    const csv = buildCsv(input({ anonymize: false }));
    expect(csv).not.toMatch(/(^|;)=HYPERLINK/m);
    expect(csv).toContain(`'=HYPERLINK`);
  });
});
