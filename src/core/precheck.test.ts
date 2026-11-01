import { describe, expect, it } from "vitest";
import { defaultChecked, prechecked } from "./precheck";

describe("recommandations cochées d'office", () => {
  it("sans risque oui, à vérifier jamais, caches d'applications jamais", () => {
    const recos = [
      { id: "cache-npm", status: "safe" as const },
      { id: "caches-applications", status: "safe" as const, precheck: false },
      { id: "fichiers-temporaires", status: "safe" as const, precheck: true },
      { id: "modeles-ollama", status: "review" as const },
    ];
    expect(defaultChecked(recos)).toEqual(["cache-npm", "fichiers-temporaires"]);
    expect(prechecked({ status: "review", precheck: true })).toBe(false);
  });
});
