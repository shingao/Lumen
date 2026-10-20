// Garde-fou de la CI pour safety/ :
// 1. chaque test listé dans src-tauri/safety/required-tests.txt existe (sous Windows : tous,
//    ailleurs : ceux marqués « all ») ;
// 2. tous les tests de safety/ s'exécutent : aucun échec, aucun ignoré, autant d'exécutés que listés ;
// 3. seul safety/ peut lever l'interdiction clippy des suppressions et des lancements de processus.
// Usage : node scripts/check-safety-tests.mjs (depuis la racine du dépôt).
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.cwd();
const tauri = join(root, "src-tauri");
const windows = process.platform === "win32";
const errors = [];

function cargo(args) {
  const r = spawnSync("cargo", args, { cwd: tauri, encoding: "utf8", shell: false, maxBuffer: 64 * 1024 * 1024 });
  if (r.error) throw r.error;
  return r;
}

// 1. Tests requis présents.
const required = readFileSync(join(tauri, "safety", "required-tests.txt"), "utf8")
  .split(/\r?\n/)
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith("#"))
  .map((l) => {
    const [scope, name] = l.split(/\s+/);
    return { scope, name };
  });
const expected = required.filter((r) => r.scope === "all" || (r.scope === "windows" && windows));

const listing = cargo(["test", "-p", "safety", "--", "--list", "--format", "terse"]);
if (listing.status !== 0) {
  console.error(listing.stderr);
  process.exit(1);
}
const listed = new Set(
  listing.stdout
    .split(/\r?\n/)
    .filter((l) => l.endsWith(": test"))
    .map((l) => l.slice(0, -": test".length)),
);
for (const { name } of expected) {
  if (!listed.has(name)) errors.push(`test requis absent : ${name}`);
}
const windowsCount = [...listed].filter((n) => n.startsWith("win::")).length;
if (windows && windowsCount === 0) errors.push("aucun test Windows compilé : le module safety::win est absent ?");

// 2. Exécution complète : aucun ignoré, aucun échec, tout ce qui est listé a tourné.
const run = cargo(["test", "-p", "safety", "--no-fail-fast", "--", "--include-ignored"]);
process.stdout.write(run.stdout);
process.stderr.write(run.stderr);
let passed = 0;
let failed = 0;
let ignored = 0;
for (const m of run.stdout.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)) {
  passed += Number(m[1]);
  failed += Number(m[2]);
  ignored += Number(m[3]);
}
const ignoredAttr = cargo(["test", "-p", "safety", "--", "--ignored", "--list", "--format", "terse"]);
const ignoredTests = ignoredAttr.stdout.split(/\r?\n/).filter((l) => l.endsWith(": test"));
if (ignoredTests.length > 0) errors.push(`tests marqués #[ignore] interdits : ${ignoredTests.join(", ")}`);
if (run.status !== 0 || failed > 0) errors.push(`${failed} test(s) de safety/ en échec`);
if (ignored > 0) errors.push(`${ignored} test(s) de safety/ ignoré(s)`);
if (passed !== listed.size) errors.push(`${passed} test(s) exécuté(s) pour ${listed.size} listé(s)`);
if (passed < expected.length) errors.push(`${passed} test(s) exécuté(s), ${expected.length} requis au minimum`);

// 3. `allow(clippy::disallowed_methods)` uniquement dans safety/.
function walk(dir, out) {
  for (const name of readdirSync(dir)) {
    if (name === "target" || name === "gen" || name === "node_modules") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".rs")) out.push(p);
  }
  return out;
}
for (const file of walk(tauri, [])) {
  const rel = relative(tauri, file).replaceAll("\\", "/");
  if (rel.startsWith("safety/")) continue;
  if (readFileSync(file, "utf8").includes("disallowed_methods")) {
    errors.push(`${rel} lève l'interdiction des suppressions : réservé à safety/`);
  }
}

console.log(
  `\nsafety/ : ${passed} test(s) exécuté(s) (${windowsCount} Windows), ${failed} échec(s), ${ignored} ignoré(s), ` +
    `${expected.length} requis sur cette plateforme.`,
);
if (errors.length > 0) {
  for (const e of errors) console.error(`ÉCHEC : ${e}`);
  process.exit(1);
}
console.log("OK : tous les tests requis de safety/ ont été exécutés.");
