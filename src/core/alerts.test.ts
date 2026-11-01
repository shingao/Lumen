import { describe, expect, it } from "vitest";
import type { Volume } from "../ipc/types";
import { dueAlerts, freeRatio, localDay, markSent } from "./alerts";

const vol = (letter: string, free: number, total: number, kind: Volume["kind"] = "fixed"): Volume => ({
  id: letter,
  key: `guid-${letter}`,
  letter,
  label: "",
  kind,
  fileSystem: "NTFS",
  mftCapable: true,
  total,
  free,
});

describe("alertes d'espace libre", () => {
  const volumes = [vol("C:", 40, 500), vol("D:", 300, 1000), vol("E:", 1, 32, "removable"), vol("Z:", 1, 1000, "network"), vol("R:", 0, 0, "other")];

  it("seulement les disques locaux sous le seuil", () => {
    expect(freeRatio(volumes[0] as Volume)).toBeCloseTo(0.08);
    expect(dueAlerts(volumes, 10, {}, "2026-10-28").map((v) => v.letter)).toEqual(["C:", "E:"]);
    expect(dueAlerts(volumes, 5, {}, "2026-10-28").map((v) => v.letter)).toEqual(["E:"]);
    expect(dueAlerts(volumes, 35, {}, "2026-10-28").map((v) => v.letter)).toEqual(["C:", "D:", "E:"]);
  });

  it("au plus une fois par jour et par disque, retenu par sa clé et non par sa lettre", () => {
    const today = "2026-10-28";
    const sent = markSent({}, ["guid-C:"], today);
    expect(dueAlerts(volumes, 10, sent, today).map((v) => v.letter)).toEqual(["E:"]);
    // Le lendemain : de nouveau, et les jours passés sont oubliés.
    expect(dueAlerts(volumes, 10, sent, "2026-10-29").map((v) => v.letter)).toEqual(["C:", "E:"]);
    expect(markSent(sent, ["guid-E:"], "2026-10-29")).toEqual({ "guid-E:": "2026-10-29" });
    // Même disque sous une autre lettre : déjà signalé.
    const moved = { ...vol("F:", 40, 500), key: "guid-C:" };
    expect(dueAlerts([moved], 10, sent, today)).toEqual([]);
  });

  it("jour local", () => {
    expect(localDay(new Date(2026, 0, 5, 23, 59).getTime())).toBe("2026-01-05");
  });
});
