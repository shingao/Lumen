import { describe, expect, it } from "vitest";
import { GIB } from "./format";
import { byteTicks, inRange, nearest, scaleOf } from "./timeline";

const DAY = 86_400_000;
const NOW = Date.UTC(2026, 9, 21, 12);

describe("courbe de la Timeline", () => {
  it("plages de dates", () => {
    const pts = [40, 20, 6, 0].map((d) => ({ at: NOW - d * DAY }));
    expect(inRange(pts, "7d", NOW)).toHaveLength(2);
    expect(inRange(pts, "30d", NOW)).toHaveLength(3);
    expect(inRange(pts, "all", NOW)).toHaveLength(4);
  });

  it("graduations rondes en Gio", () => {
    const { lo, hi, ticks } = byteTicks(183.4 * GIB, 231.2 * GIB);
    expect(lo).toBe(180 * GIB);
    expect(hi).toBe(240 * GIB);
    expect(ticks.map((t) => t / GIB)).toEqual([180, 200, 220, 240]);
    // Série plate : au moins un intervalle.
    const flat = byteTicks(10 * GIB, 10 * GIB);
    expect(flat.hi).toBeGreaterThan(flat.lo);
  });

  it("échelles et point le plus proche", () => {
    const pts = [
      { at: NOW - 10 * DAY, used: 100 * GIB },
      { at: NOW, used: 120 * GIB },
    ];
    const s = scaleOf(pts, { left: 40, top: 10, width: 600, height: 200 }, NOW - 10 * DAY, NOW);
    expect(s.x(NOW - 10 * DAY)).toBe(40);
    expect(s.x(NOW)).toBe(640);
    expect(s.y(120 * GIB)).toBeLessThan(s.y(100 * GIB));
    expect(s.y(s.ticks[0] ?? 0)).toBe(210);
    expect(nearest(pts, NOW - 2 * DAY)?.used).toBe(120 * GIB);
    expect(nearest([], NOW)).toBeNull();
  });
});
