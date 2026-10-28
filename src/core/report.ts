// Rapport d'analyse exporté : une page HTML autonome dans la DA de Lumen (aucune ressource
// externe : styles, polices et images embarqués, aucun script) et un CSV. Fait pour être
// partagé : les chemins sont anonymisés par défaut (voir anonymize.ts). Logique pure : les
// textes, les styles et les polices sont fournis par l'appelant.

import { anonymizeName, anonymizePath } from "./anonymize";
import { formatBytes, formatCount, formatDate, formatDateTime, type Locale } from "./format";
import { squarify, type Rect } from "./treemap/squarify";
import type { Category, Kpis, Recommendation, TreeNode } from "../ipc/types";

/** Textes traduits par l'appelant (clés de i18n). */
export type ReportText = (key: string, params?: Record<string, string | number>) => string;

export interface ReportInput {
  locale: Locale;
  text: ReportText;
  version: string;
  generatedAt: number;
  /** « Dossier utilisateur », « Disque C: »… */
  targetLabel: string;
  rootPath: string;
  finishedAt: number;
  method: "walk" | "mft";
  files: number;
  dirs: number;
  /** Racine de l'analyse avec ses deux premiers niveaux. */
  root: TreeNode;
  kpis: Kpis | null;
  recommendations: Recommendation[];
  /** Espace utilisé dans le temps (Timeline), le plus ancien d'abord. */
  timeline: { at: number; used: number }[];
  anonymize: boolean;
  profile: string | null;
}

export interface ReportAssets {
  /** Variables de la DA (tokens.css). */
  tokens: string;
  /** Polices en data URL (woff2). */
  sans: string;
  mono: string;
  dark: boolean;
}

const CATEGORIES: readonly Category[] = ["media", "code", "packages", "tools", "android", "ai", "games", "system"];

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c);
}

function pathOf(input: ReportInput, path: string, file: boolean): string {
  return input.anonymize ? anonymizePath(path, { profile: input.profile, file }) : path;
}

function nameOf(input: ReportInput, node: TreeNode): string {
  if (node.othersCount !== null) return input.text("report.others", { n: node.othersCount });
  return input.anonymize ? anonymizeName(node.path, node.name, { profile: input.profile, file: node.kind === "file" || node.kind === "cloudFile" }) : node.name;
}

const W = 1000;
const H = 560;

function pct(v: number, of: number): string {
  return `${((v / of) * 100).toFixed(3)}%`;
}

function tile(input: ReportInput, n: TreeNode, r: Rect, depth: number): string {
  const size = formatBytes(n.size, input.locale);
  const name = nameOf(input, n);
  const fill = n.othersCount !== null || n.category === "system" ? "keep" : n.category;
  const safe = n.status === "safe" && n.othersCount === null;
  const cls = `tile${safe ? " hatch" : ""}${depth === 2 ? " sub" : ""}`;
  const style = `left:${pct(r.x, W)};top:${pct(r.y, H)};width:${pct(r.w, W)};height:${pct(r.h, H)};background-color:var(--${fill === "keep" ? "st-keep" : `cat-${fill}`})`;
  const big = r.w > 90 && r.h > 34;
  const label = big ? `<span class="tname">${escapeHtml(name)}</span><span class="tsize">${escapeHtml(size)}</span>` : "";
  return `<div class="${cls}" style="${style}" title="${escapeHtml(`${name} — ${size}`)}">${label}</div>`;
}

/** Treemap statique (2 niveaux), positions en pourcentage. */
export function treemapHtml(input: ReportInput): string {
  const value = (n: TreeNode) => n.layoutSize ?? n.size;
  const top = squarify(input.root.children ?? [], value, { x: 0, y: 0, w: W, h: H }, 3);
  const out: string[] = [];
  for (const p of top) {
    const kids = p.item.children ?? [];
    // Deux niveaux : les enfants d'une grande tuile, sous son titre.
    if (kids.length > 0 && p.rect.w > 120 && p.rect.h > 90) {
      out.push(`<div class="group" style="left:${pct(p.rect.x, W)};top:${pct(p.rect.y, H)};width:${pct(p.rect.w, W)};height:${pct(p.rect.h, H)}"></div>`);
      const head = 22;
      const inner = { x: p.rect.x + 3, y: p.rect.y + head, w: p.rect.w - 6, h: p.rect.h - head - 3 };
      out.push(
        `<div class="ghead" style="left:${pct(p.rect.x, W)};top:${pct(p.rect.y, H)};width:${pct(p.rect.w, W)}"><span class="tname">${escapeHtml(nameOf(input, p.item))}</span><span class="tsize">${escapeHtml(formatBytes(p.item.size, input.locale))}</span></div>`,
      );
      for (const c of squarify(kids, value, inner, 3)) out.push(tile(input, c.item, c.rect, 2));
    } else {
      out.push(tile(input, p.item, p.rect, 1));
    }
  }
  return `<div class="treemap" role="img" aria-label="${escapeHtml(input.text("report.treemap"))}">${out.join("")}</div>`;
}

/** Courbe de l'espace utilisé (SVG en ligne). */
export function timelineSvg(input: ReportInput): string {
  const pts = input.timeline;
  if (pts.length < 2) return `<p class="muted">${escapeHtml(input.text("report.noHistory"))}</p>`;
  const [w, h, pad] = [640, 160, 8];
  const t0 = pts[0]?.at ?? 0;
  const t1 = pts[pts.length - 1]?.at ?? 1;
  const min = Math.min(...pts.map((p) => p.used));
  const max = Math.max(...pts.map((p) => p.used));
  const x = (t: number) => pad + ((t - t0) / Math.max(1, t1 - t0)) * (w - 2 * pad);
  const y = (u: number) => h - pad - ((u - min) / Math.max(1, max - min)) * (h - 2 * pad);
  const line = pts.map((p) => `${x(p.at).toFixed(1)},${y(p.used).toFixed(1)}`).join(" ");
  const first = pts[0]?.used ?? 0;
  const last = pts[pts.length - 1]?.used ?? 0;
  return `<svg class="curve" viewBox="0 0 ${w} ${h}" role="img" aria-label="${escapeHtml(input.text("report.timeline"))}"><polyline points="${line}" fill="none" stroke="var(--text-1)" stroke-width="2" stroke-linejoin="round"/></svg>
<div class="axis"><span>${escapeHtml(formatDate(t0, input.locale))} · ${escapeHtml(formatBytes(first, input.locale))}</span><span>${escapeHtml(formatDate(t1, input.locale))} · ${escapeHtml(formatBytes(last, input.locale))}</span></div>`;
}

const CSS = `
@font-face{font-family:"Inter Variable";font-style:normal;font-weight:100 900;src:url(__SANS__) format("woff2")}
@font-face{font-family:"JetBrains Mono";font-style:normal;font-weight:400 500;src:url(__MONO__) format("woff2")}
*{box-sizing:border-box}
body{margin:0;background:var(--bg-app);color:var(--text-1);font-family:var(--font-sans);font-size:var(--fs-13);line-height:1.45}
main{max-width:1080px;margin:0 auto;padding:32px 24px}
h1{font-size:var(--fs-18);font-weight:var(--fw-semibold);letter-spacing:var(--ls-title);margin:0}
h2{font-size:var(--fs-15);font-weight:var(--fw-semibold);letter-spacing:var(--ls-heading);margin:0 0 12px}
.box{background:var(--bg-container);border:1px solid var(--border);border-radius:var(--r-container);padding:20px;margin-top:16px}
.head{display:flex;justify-content:space-between;align-items:flex-end;gap:16px;flex-wrap:wrap}
.muted,.sub{color:var(--text-label)}
.mono,td.num,.tsize,.path{font-family:var(--font-mono)}
.kpis{display:grid;grid-template-columns:repeat(4,1fr)}
.kpi{padding:4px 16px;border-left:1px solid var(--border-soft)}
.kpi:first-child{border-left:0;padding-left:0}
.kpi .v{font-size:var(--fs-24);font-weight:var(--fw-semibold);font-family:var(--font-mono);letter-spacing:var(--ls-tight)}
.kpi .sub,.kpi .muted{font-size:var(--fs-12)}
.dot{display:inline-block;width:8px;height:8px;border-radius:var(--r-pill);margin-right:6px;vertical-align:middle}
.dot.safe{background:var(--st-safe-dot)}.dot.review{background:var(--st-review-dot)}
.treemap{position:relative;width:100%;aspect-ratio:${W}/${H}}
.tile,.group,.ghead{position:absolute}
.tile{border-radius:var(--r-tile);padding:6px 8px;overflow:hidden;display:flex;flex-direction:column;justify-content:space-between;color:var(--text-1)}
.tile.sub{border-radius:var(--r-tile-child)}
.group{border-radius:var(--r-tile);background:var(--bg-sunken)}
.ghead{display:flex;justify-content:space-between;gap:8px;padding:4px 8px}
.hatch{background-image:var(--hatch)}
.tname{font-weight:var(--fw-medium);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-size:var(--fs-12)}
.tsize{font-size:var(--fs-11);color:var(--text-2)}
.legend{display:flex;flex-wrap:wrap;gap:12px;margin-top:12px;font-size:var(--fs-12);color:var(--text-label)}
.sw{display:inline-block;width:12px;height:12px;border-radius:var(--r-swatch);border:1px solid var(--swatch-border);margin-right:6px;vertical-align:-2px}
table{width:100%;border-collapse:collapse}
th{text-align:left;font-weight:var(--fw-medium);color:var(--text-label);font-size:var(--fs-12);padding:6px 8px;border-bottom:1px solid var(--border)}
td{padding:8px;border-bottom:1px solid var(--border-soft);vertical-align:top}
td.num{text-align:right;white-space:nowrap}
.path{font-size:var(--fs-11-5);color:var(--text-label);word-break:break-all}
.curve{width:100%;height:auto;display:block}
.axis{display:flex;justify-content:space-between;font-family:var(--font-mono);font-size:var(--fs-11-5);color:var(--text-label)}
footer{margin-top:24px;font-size:var(--fs-12);color:var(--text-label)}
@media print{body{background:var(--bg-container)}.box{break-inside:avoid}}
`;

/** Page HTML autonome du rapport. */
export function buildReportHtml(input: ReportInput, assets: ReportAssets): string {
  const tx = input.text;
  const L = input.locale;
  const size = (n: number) => escapeHtml(formatBytes(n, L));
  const k = input.kpis;
  const kpi = (label: string, value: string, sub: string, dot = "") =>
    `<div class="kpi"><div class="muted">${dot ? `<span class="dot ${dot}"></span>` : ""}${escapeHtml(label)}</div><div class="v">${value}</div><div class="sub">${escapeHtml(sub)}</div></div>`;
  const kpis = k
    ? `<div class="kpis">${kpi(tx("kpi.freeAfter"), size(k.freeAfter), tx("kpi.freeToday", { size: formatBytes(k.freeNow, L) }))}${kpi(
        tx("kpi.safe"),
        k.rulesReady ? size(k.safeBytes) : "—",
        tx("report.count", { n: formatCount(k.safeCount, L) }),
        "safe",
      )}${kpi(tx("kpi.review"), k.rulesReady ? size(k.reviewBytes) : "—", tx("report.count", { n: formatCount(k.reviewCount, L) }), "review")}${kpi(
        tx("kpi.unanalyzed"),
        size(k.unanalyzed),
        tx("kpi.unanalyzedSub"),
      )}</div>`
    : "";
  const recos = input.recommendations.length
    ? `<table><thead><tr><th>${escapeHtml(tx("report.col.status"))}</th><th>${escapeHtml(tx("report.col.name"))}</th><th style="text-align:right">${escapeHtml(tx("report.col.size"))}</th></tr></thead><tbody>${input.recommendations
        .map(
          (r) =>
            `<tr><td><span class="dot ${r.status}"></span>${escapeHtml(tx(`status.${r.status}`))}</td><td>${escapeHtml(r.name)}<div class="path">${escapeHtml(pathOf(input, r.path, false))}</div></td><td class="num">${size(r.size)}</td></tr>`,
        )
        .join("")}</tbody></table>`
    : `<p class="muted">${escapeHtml(tx("report.noRecos"))}</p>`;
  const present = new Set((input.root.children ?? []).flatMap((c) => [c.category, ...(c.children ?? []).map((g) => g.category)]));
  const legend = CATEGORIES.filter((c) => present.has(c))
    .map((c) => `<span><span class="sw" style="background:var(--${c === "system" ? "st-keep" : `cat-${c}`})"></span>${escapeHtml(tx(`category.${c}`))}</span>`)
    .concat(`<span><span class="sw hatch" style="background-color:var(--st-safe)"></span>${escapeHtml(tx("legend.hatch"))}</span>`)
    .join("");
  const css = CSS.replace("__SANS__", assets.sans).replace("__MONO__", assets.mono);
  const title = tx("report.title");
  return `<!DOCTYPE html>
<html lang="${L}" data-theme="${assets.dark ? "dark" : "light"}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; font-src data:; img-src data:">
<meta name="generator" content="Lumen ${escapeHtml(input.version)}">
<title>${escapeHtml(`${title} — ${input.targetLabel}`)}</title>
<style>${assets.tokens}${css}</style>
</head>
<body>
<main>
<header class="head"><div><h1>${escapeHtml(title)}</h1><div class="muted">${escapeHtml(input.targetLabel)} · <span class="mono">${escapeHtml(pathOf(input, input.rootPath, false))}</span></div></div>
<div class="muted">${escapeHtml(tx("report.analysed", { date: formatDateTime(input.finishedAt, L) }))} · ${escapeHtml(tx(`report.method.${input.method}`))} · ${escapeHtml(
    tx("report.items", { files: formatCount(input.files, L), dirs: formatCount(input.dirs, L) }),
  )}</div></header>
<section class="box">${kpis}</section>
<section class="box"><h2>${escapeHtml(tx("report.treemap"))} · <span class="mono">${size(input.root.size)}</span></h2>${treemapHtml(input)}<div class="legend">${legend}</div></section>
<section class="box"><h2>${escapeHtml(tx("report.recommendations"))}</h2>${recos}</section>
<section class="box"><h2>${escapeHtml(tx("report.timeline"))}</h2>${timelineSvg(input)}</section>
<footer>${escapeHtml(tx("report.footer", { version: input.version, date: formatDateTime(input.generatedAt, L) }))}${
    input.anonymize ? ` ${escapeHtml(tx("report.anonymized"))}` : ""
  }</footer>
</main>
</body>
</html>
`;
}

function csvCell(v: string | number, sep: string): string {
  const s = String(v);
  // Jamais une formule interprétée par un tableur (=, +, -, @ en tête).
  const safe = /^[=+\-@\t\r]/.test(s) ? `'${s}` : s;
  return safe.includes(sep) || safe.includes('"') || safe.includes("\n") ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** CSV des éléments de l'analyse (racine et ses niveaux reçus) : `;` en français, `,` en anglais. */
export function buildCsv(input: ReportInput): string {
  const sep = input.locale === "fr" ? ";" : ",";
  const tx = input.text;
  const head = ["path", "name", "kind", "allocated", "logical", "category", "status", "gain", "lastOpened"].map((c) => tx(`report.csv.${c}`));
  const rows: string[] = [head.map((h) => csvCell(h, sep)).join(sep)];
  const walk = (n: TreeNode, depth: number) => {
    if (depth > 0 && n.othersCount === null) {
      const file = n.kind === "file" || n.kind === "cloudFile";
      rows.push(
        [
          pathOf(input, n.path, file),
          nameOf(input, n),
          n.kind,
          n.size,
          n.logical,
          n.category,
          n.status,
          n.gain,
          n.lastOpened > 0 ? new Date(n.lastOpened).toISOString().slice(0, 10) : "",
        ]
          .map((c) => csvCell(c, sep))
          .join(sep),
      );
    }
    for (const c of n.children ?? []) walk(c, depth + 1);
  };
  walk(input.root, 0);
  // BOM : Excel lit l'UTF-8 sans se tromper d'encodage.
  return `\uFEFF${rows.join("\r\n")}\r\n`;
}
