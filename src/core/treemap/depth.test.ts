import { describe, expect, it } from "vitest";
import { nests } from "./depth";

describe("profondeur adaptative", () => {
  it("une tuile d'au moins 120 × 80 px montre ses enfants, jusqu'au 4e niveau", () => {
    expect(nests(3, 2, 120, 80)).toBe(true);
    expect(nests(3, 3, 400, 300)).toBe(true);
    expect(nests(3, 4, 400, 300)).toBe(false);
    expect(nests(3, 2, 119, 300)).toBe(false);
    expect(nests(3, 2, 300, 79)).toBe(false);
    expect(nests(0, 2, 400, 300)).toBe(false);
  });
});
