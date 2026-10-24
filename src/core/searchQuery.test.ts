import { describe, expect, it } from "vitest";
import { matchesFilters, parseQuery, parseSize } from "./searchQuery";

describe("filtres de la recherche", () => {
  it("extension, taille, type, dossier, disque ; le reste est cherché dans les noms", () => {
    const q = parseQuery("mariage ext:.MP4,mkv >1go <=2,5go type:vidéo disque:d:");
    expect(q.words).toEqual(["mariage"]);
    expect(q.exts).toEqual(["mp4", "mkv"]);
    expect(q.minSize).toBe(1024 ** 3);
    expect(q.maxSize).toBe(Math.round(2.5 * 1024 ** 3));
    expect(q.types).toEqual(["video"]);
    expect(q.drives).toEqual(["D"]);
    expect(parseQuery("type:dossier").dirsOnly).toBe(true);
    // Filtre illisible : un mot comme un autre.
    expect(parseQuery("type:truc >beaucoup").words).toEqual(["type:truc", ">beaucoup"]);
    expect(parseSize("500mo")).toBe(500 * 1024 ** 2);
    expect(parseSize("12")).toBe(12);
    expect(parseSize("x")).toBeNull();
  });

  it("un élément passe ou non les filtres", () => {
    const film = { name: "Mariage.mp4", size: 9 * 1024 ** 3, kind: "file" as const, drive: "D:" };
    expect(matchesFilters(parseQuery("ext:mp4 >1go disque:D"), film)).toBe(true);
    expect(matchesFilters(parseQuery("type:image"), film)).toBe(false);
    expect(matchesFilters(parseQuery("disque:C"), film)).toBe(false);
    expect(matchesFilters(parseQuery("type:dossier"), film)).toBe(false);
    expect(matchesFilters(parseQuery("<1go"), film)).toBe(false);
  });
});
