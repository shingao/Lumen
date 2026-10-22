import { describe, expect, it } from "vitest";
import type { DevProject, Game } from "../ipc/types";
import { sortGames, sortProjects } from "./devgames";

const game = (name: string, size: number | null, lastPlayed: number | null): Game => ({
  id: name, launcher: "steam", name, installDir: null, size, sizeSource: "launcher", drive: "C:", lastPlayed, status: "keep", cover: false, open: "uri", canMove: true, data: null,
});

const project = (name: string, lastActivity: number | null, artifactsSize: number): DevProject => ({
  id: 0, name, path: name, language: null, lastActivity, activitySource: "files", git: false, lockfile: true, sourceSize: 0, gitSize: 0, artifactsSize, artifacts: [],
});

describe("tris Jeux et Dev", () => {
  it("jeux : taille, dernière partie (inconnue en dernier), nom", () => {
    const list = [game("b", 10, null), game("a", 30, 200), game("c", null, 100)];
    expect(sortGames(list, "size", "fr").map((g) => g.name)).toEqual(["a", "b", "c"]);
    expect(sortGames(list, "lastPlayed", "fr").map((g) => g.name)).toEqual(["c", "a", "b"]);
    expect(sortGames(list, "name", "fr").map((g) => g.name)).toEqual(["a", "b", "c"]);
  });

  it("projets : inactif depuis le plus longtemps d'abord, ou plus gros artefacts", () => {
    const list = [project("x", 300, 5), project("y", 100, 5), project("z", null, 9)];
    expect(sortProjects(list, "inactive").map((p) => p.name)).toEqual(["y", "x", "z"]);
    expect(sortProjects(list, "artifacts").map((p) => p.name)).toEqual(["z", "x", "y"]);
  });
});
