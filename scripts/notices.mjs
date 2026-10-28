// THIRD_PARTY_NOTICES.txt : licences de tout ce que Lumen embarque, généré (jamais écrit à la main).
//  - crates Rust de l'exécutable Windows : cargo-about (src-tauri/about.toml), sans les
//    dépendances de développement ni de compilation ;
//  - paquets npm embarqués dans l'interface : dépendances de production de package.json et les
//    leurs, avec le fichier de licence de chaque paquet (polices comprises).
// Résumé (nombre de composants) dans src/assets/notices-summary.json, affiché dans À propos.
//
//   node scripts/notices.mjs           régénère les deux fichiers
//   node scripts/notices.mjs --check   échoue s'ils ne sont pas à jour (CI)

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(root, "THIRD_PARTY_NOTICES.txt");
const SUMMARY = join(root, "src", "assets", "notices-summary.json");
const RULE = "=".repeat(72);
const THIN = "-".repeat(72);

/** Texte comparable : fins de ligne, espaces de fin, lignes vides au bord. */
const clean = (text) =>
  text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((l) => l.trimEnd())
    .join("\n")
    .trim();

function rustLicenses() {
  // Vers un fichier (`-o`) : sous PowerShell, cargo-about refuse d'écrire sur la sortie standard.
  const dir = mkdtempSync(join(tmpdir(), "lumen-notices-"));
  const file = join(dir, "about.json");
  let out;
  try {
    execFileSync("cargo", ["about", "generate", "--format", "json", "-o", file, "--manifest-path", join(root, "src-tauri", "Cargo.toml")], {
      cwd: join(root, "src-tauri"),
      stdio: ["ignore", "inherit", "inherit"],
    });
    out = readFileSync(file, "utf8");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  const data = JSON.parse(out);
  const crates = new Set();
  const groups = data.licenses.map((l) => {
    const users = l.used_by.map((u) => `${u.crate.name} ${u.crate.version}`);
    users.forEach((u) => crates.add(u));
    return { license: l.id, users, text: clean(l.text) };
  });
  return { groups, count: crates.size };
}

const LICENSE_FILE = /^(licen[cs]e|copying|ofl)([.-].*)?$/i;

function npmLicenses() {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const seen = new Map();
  const visit = (name, from) => {
    // Résolution de Node : node_modules du paquet parent, puis ceux de la racine.
    const candidates = [join(from, "node_modules", name), join(root, "node_modules", name)];
    const dir = candidates.find((d) => existsSync(join(d, "package.json")));
    if (!dir) throw new Error(`paquet introuvable : ${name}`);
    const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const id = `${meta.name} ${meta.version}`;
    if (seen.has(id)) return;
    const file = readdirSync(dir).find((f) => LICENSE_FILE.test(f));
    if (!file) throw new Error(`${id} : aucun fichier de licence`);
    const license = typeof meta.license === "string" ? meta.license : (meta.license?.type ?? "voir le texte");
    seen.set(id, { license, text: clean(readFileSync(join(dir, file), "utf8")) });
    for (const dep of Object.keys(meta.dependencies ?? {})) visit(dep, dir);
  };
  for (const dep of Object.keys(pkg.dependencies ?? {})) visit(dep, root);
  // Paquets regroupés par texte de licence identique.
  const byText = new Map();
  for (const [id, { license, text }] of [...seen].sort(([a], [b]) => a.localeCompare(b))) {
    const key = `${license}\n${text}`;
    if (!byText.has(key)) byText.set(key, { license, users: [], text });
    byText.get(key).users.push(id);
  }
  return { groups: [...byText.values()], count: seen.size };
}

function section(title, groups) {
  const blocks = [...groups]
    .sort((a, b) => a.license.localeCompare(b.license) || a.users[0].localeCompare(b.users[0]))
    .map((g) => [RULE, `Licence / License : ${g.license}`, `Utilisée par / Used by : ${[...g.users].sort().join(", ")}`, THIN, g.text].join("\n"));
  return [`${RULE}\n${title}\n${RULE}`, ...blocks].join("\n\n");
}

const rust = rustLicenses();
const npm = npmLicenses();
const text =
  [
    "Lumen — composants tiers / third-party components",
    "Généré par scripts/notices.mjs (cargo-about et licences npm) : ne pas modifier à la main.",
    `Crates Rust : ${rust.count} · Paquets npm : ${npm.count}`,
    section("Crates Rust (exécutable Windows)", rust.groups),
    section("Paquets npm (interface)", npm.groups),
  ].join("\n\n") + "\n";
const summary = `${JSON.stringify({ rust: rust.count, npm: npm.count })}\n`;

if (process.argv.includes("--check")) {
  const same = (path, content) => existsSync(path) && readFileSync(path, "utf8") === content;
  if (!same(OUT, text) || !same(SUMMARY, summary)) {
    console.error("THIRD_PARTY_NOTICES.txt n'est pas à jour : lancer `npm run notices` et committer le résultat.");
    process.exit(1);
  }
  console.log(`THIRD_PARTY_NOTICES.txt à jour (${rust.count} crates, ${npm.count} paquets npm).`);
} else {
  writeFileSync(OUT, text);
  writeFileSync(SUMMARY, summary);
  console.log(`THIRD_PARTY_NOTICES.txt : ${rust.count} crates Rust, ${npm.count} paquets npm.`);
}
