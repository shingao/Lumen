import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("recherche floue", () => {
  it("trouve une sous-séquence, insensible à la casse et aux accents", () => {
    expect(fuzzyScore("telech", "Téléchargements")).not.toBeNull();
    expect(fuzzyScore("ndmd", "node_modules")).not.toBeNull();
    expect(fuzzyScore("xyz", "node_modules")).toBeNull();
  });

  it("classe les correspondances contiguës et en début de mot d'abord", () => {
    const items = ["AppData\\Local\\Temp", "Templates", "attempt.txt"];
    expect(fuzzyFilter(items, "temp", (s) => s, 3)[0]).toBe("Templates");
  });
});
