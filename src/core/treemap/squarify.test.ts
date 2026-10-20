import { describe, expect, it } from "vitest";
import { squarify } from "./squarify";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("couvre le rectangle et respecte les proportions", () => {
    const values = [6, 6, 4, 3, 2, 2, 1];
    const placed = squarify(values, (v) => v, { x: 0, y: 0, w: 600, h: 400 });
    expect(placed).toHaveLength(values.length);
    const total = placed.reduce((s, p) => s + area(p.rect), 0);
    expect(Math.abs(total - 600 * 400)).toBeLessThan(600 * 400 * 0.01);
    for (const p of placed) {
      expect(area(p.rect) / total).toBeCloseTo(p.item / 24, 1);
      expect(p.rect.x).toBeGreaterThanOrEqual(0);
      expect(p.rect.y).toBeGreaterThanOrEqual(0);
      expect(p.rect.x + p.rect.w).toBeLessThanOrEqual(600);
      expect(p.rect.y + p.rect.h).toBeLessThanOrEqual(400);
    }
  });

  it("produit des tuiles plutôt carrées", () => {
    const placed = squarify([10, 10, 10, 10], (v) => v, { x: 0, y: 0, w: 400, h: 400 });
    for (const p of placed) expect(Math.max(p.rect.w / p.rect.h, p.rect.h / p.rect.w)).toBeLessThan(1.5);
  });

  it("laisse exactement `gap` entre voisines et colle aux bords", () => {
    const placed = squarify([1, 1], (v) => v, { x: 0, y: 0, w: 203, h: 100 }, 3);
    const [a, b] = placed.map((p) => p.rect).sort((p, q) => p.x - q.x);
    expect(a?.x).toBe(0);
    expect((b?.x ?? 0) - ((a?.x ?? 0) + (a?.w ?? 0))).toBe(3);
    expect((b?.x ?? 0) + (b?.w ?? 0)).toBe(203);
  });

  it("ignore les valeurs nulles et les rectangles vides", () => {
    expect(squarify([0, -1], (v) => v, { x: 0, y: 0, w: 10, h: 10 })).toEqual([]);
    expect(squarify([1], (v) => v, { x: 0, y: 0, w: 0, h: 10 })).toEqual([]);
  });

  it("trie du plus grand au plus petit", () => {
    const placed = squarify([1, 5, 3], (v) => v, { x: 0, y: 0, w: 300, h: 200 });
    expect(placed.map((p) => p.item)).toEqual([5, 3, 1]);
  });
});
