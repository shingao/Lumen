/// <reference types="node" />
// Contraste AA (4,5:1) des textes de l'interface, dans les deux thèmes, lu directement dans tokens.css.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { contrastRatio, over, parseColor, type Rgba } from "../core/contrast";

// Fichier de test exécuté par Node : seule exception aux types navigateur de l'appli.
const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

function block(selector: string): Map<string, string> {
  const start = css.indexOf(`${selector} {`);
  const end = css.indexOf("\n}", start);
  const map = new Map<string, string>();
  for (const m of css.slice(start, end).matchAll(/(--[\w-]+):\s*([^;]+);/g)) map.set(m[1] as string, (m[2] as string).trim());
  return map;
}

const light = block(":root");
const themes = { clair: light, sombre: new Map([...light, ...block('[data-theme="dark"]')]) };

const CATS = ["--cat-media", "--cat-code", "--cat-packages", "--cat-tools", "--cat-android", "--cat-ai", "--cat-games"];
const STATUS = ["--st-safe", "--st-review", "--st-keep"];
const AA = 4.5;

describe.each(Object.entries(themes))("contraste AA, thème %s", (_name, tokens) => {
  const color = (name: string): Rgba => {
    const v = tokens.get(name);
    if (!v) throw new Error(`jeton absent : ${name}`);
    return parseColor(v);
  };
  const container = color("--bg-container");
  const solid = (name: string) => over(color(name), container);
  // Sous-tuile : voile posé sur la couleur de la tuile ; un voile de plus par niveau (3 et 4).
  const veiled = (name: string) => over(color("--tile-veil"), solid(name));
  const deeper = (name: string, levels: number) => {
    let bg = solid(name);
    for (let i = 0; i < levels; i++) bg = over(color("--tile-veil"), bg);
    return bg;
  };

  const check = (fgName: string, bg: Rgba, where: string) => {
    const ratio = contrastRatio(color(fgName), bg);
    expect(ratio, `${fgName} sur ${where} : ${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA);
  };

  it("noms, tailles et gains sur les couleurs de catégorie et de statut", () => {
    for (const bg of [...CATS, ...STATUS]) {
      for (const fg of ["--text-1", "--text-2", "--text-3", "--text-4", "--delta-safe", "--delta-review"]) {
        check(fg, solid(bg), bg);
        check(fg, veiled(bg), `${bg} + voile`);
        check(fg, deeper(bg, 2), `${bg} + 2 voiles (niveau 3)`);
        check(fg, deeper(bg, 3), `${bg} + 3 voiles (niveau 4)`);
      }
    }
  });

  it("mode Âge : textes normaux sur les paliers récents, ton fort sur les anciens", () => {
    for (const i of [0, 1, 2]) {
      for (const fg of ["--text-2", "--text-3", "--text-4", "--delta-safe", "--delta-review"]) {
        check(fg, solid(`--age-${i}`), `--age-${i}`);
        check(fg, veiled(`--age-${i}`), `--age-${i} + voile`);
      }
    }
    for (const i of [3, 4]) {
      for (const fg of ["--text-1", "--text-2"]) {
        check(fg, solid(`--age-${i}`), `--age-${i}`);
        check(fg, veiled(`--age-${i}`), `--age-${i} + voile`);
      }
    }
  });

  it("textes de l'interface sur les surfaces", () => {
    for (const bg of ["--bg-container", "--bg-app"]) {
      for (const fg of ["--text-1", "--text-2", "--text-3", "--text-4", "--text-label", "--delta-safe", "--delta-review"]) {
        check(fg, solid(bg), bg);
      }
    }
    for (const fg of ["--text-1", "--text-label-sunken"]) check(fg, solid("--bg-sunken"), "--bg-sunken");
    // Nettoyage et journal : chemins, tailles et détails sur la surface en creux.
    for (const fg of ["--text-2", "--text-3"]) check(fg, solid("--bg-sunken"), "--bg-sunken");
  });

  it("boutons", () => {
    check("--btn-primary-fg", solid("--btn-primary-bg"), "--btn-primary-bg");
    check("--btn-clean-fg", solid("--btn-clean-bg"), "--btn-clean-bg");
  });
});
