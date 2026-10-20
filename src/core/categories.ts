// Catégorie provisoire d'un dossier d'après son nom, en attendant le moteur de règles (jalon 4) :
// donne à la carte les couleurs de la maquette sur une vraie analyse. Un enfant hérite de la
// catégorie de son parent, sauf s'il en a une lui-même.

import type { Category } from "../ipc/types";

const NAMES: Record<string, Category> = {};

function add(category: Category, names: string[]): void {
  for (const n of names) NAMES[n.toLowerCase()] = category;
}

add("media", [
  "Documents", "Mes documents", "Vidéos", "Videos", "Images", "Pictures", "Musique", "Music",
  "Téléchargements", "Downloads", "Bureau", "Desktop", "OneDrive", "Photos", "Pellicule", "Camera Roll",
]);
add("code", ["Projets", "Projects", "source", "repos", "dev", "src", "workspace", "code", "git"]);
add("packages", [
  "npm-cache", ".npm", ".pnpm-store", "pnpm", ".yarn", "Yarn", ".gradle", ".m2", ".cargo", ".rustup",
  ".nuget", "NuGet", "pip", ".cache", "uv", "node_modules", "Composer", ".bun",
]);
add("android", ["Android", ".android", "Sdk", "AndroidStudio"]);
add("ai", [".ollama", "ollama", "huggingface", ".lmstudio", "LM Studio", "models"]);
add("games", ["Steam", "SteamLibrary", "Epic Games", "GOG Galaxy", "Battle.net", "EA Games", "Ubisoft", "My Games", "Riot Games"]);

export function categoryOf(name: string, inherited: Category): Category {
  return NAMES[name.toLowerCase()] ?? inherited;
}
