// Captures de l'interface (coffre factice) pour la comparer à design/maquettes/.
// Usage : npm run dev (port 1420) puis npm run captures [dossier de sortie].
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "playwright";

const out = process.argv[2] ?? "design/captures";
const url = process.env.LUMEN_URL ?? "http://localhost:1420/";
const executablePath = process.env.LUMEN_CHROMIUM || undefined;
mkdirSync(out, { recursive: true });

const browser = await chromium.launch({ executablePath });

// Tutoriel et astuces (jalon 10) déjà vus, sauf pour les captures qui les montrent (`fresh`).
const SCREENS = ["analyse", "timeline", "cleanup", "dev", "games", "journal"];
async function welcomed(page, { onboarding = true, tips = SCREENS } = {}) {
  await page.addInitScript(
    ([done, seen]) => {
      const key = "lumen-dev-settings";
      const raw = globalThis.localStorage.getItem(key);
      const s = raw ? JSON.parse(raw) : {};
      if (s.onboardingDone === undefined) s.onboardingDone = done;
      if (s.tipsSeen === undefined) s.tipsSeen = seen;
      globalThis.localStorage.setItem(key, JSON.stringify(s));
    },
    [onboarding, tips],
  );
}

async function shot(name, { theme = "light", locale = "fr-FR", width = 1440, height = 1024, act, fresh, reducedMotion = "no-preference" } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, locale, colorScheme: theme, reducedMotion });
  await welcomed(page, fresh);
  await page.goto(url);
  await page.waitForSelector("#root > *");
  await page.evaluate(() => globalThis.document.fonts.ready);
  if (act) await act(page);
  await page.waitForTimeout(350);
  await page.screenshot({ path: join(out, `${name}.png`) });
  await page.close();
}

const hoverNpm = async (page) => {
  const tile = page.locator("[data-node]", { hasText: "npm-cache" }).first();
  const box = await tile.boundingBox();
  if (box) await page.mouse.move(box.x + box.width * 0.35, box.y + box.height * 0.4);
};
const openNpm = (page) => page.getByRole("button", { name: /Cache npm|npm cache/ }).first().click();

await shot("01-principal-clair", { act: hoverNpm });
await shot("02-detail-clair", { act: openNpm });
await shot("03-principal-sombre", { theme: "dark", act: hoverNpm });
await shot("04-detail-sombre", { theme: "dark", act: openNpm });
await shot("05-nettoyage", { act: (p) => p.getByRole("button", { name: "Nettoyage", exact: true }).first().click() });
await shot("06-age-sombre", { theme: "dark", act: (p) => p.getByRole("button", { name: "Âge" }).click() });
await shot("07-principal-anglais", { locale: "en-US", act: hoverNpm });
await shot("08-parametres", { act: (p) => p.getByRole("button", { name: "Paramètres" }).click() });
await shot("09-palette", { act: (p) => p.keyboard.press("Control+K").then(() => p.keyboard.type("grad")) });
await shot("10-timeline-sombre", { theme: "dark", act: (p) => p.getByRole("button", { name: "Timeline", exact: true }).click() });
await shot("11-fenetre-etroite", { width: 1100, height: 900 });
// Jalon 5 : confirmation, seconde confirmation du définitif, bilan, journal.
const clean = (p) => p.getByRole("button", { name: "Nettoyer", exact: true }).click();
await shot("12-confirmation-clair", {
  act: async (p) => {
    await clean(p);
    await p.getByText("Voir la liste").click();
  },
});
await shot("13-confirmation-sombre", { theme: "dark", act: clean });
await shot("14-definitif", {
  act: async (p) => {
    await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-settings", JSON.stringify({ permanentDelete: true })));
    await p.reload();
    await p.waitForSelector("#root > *");
    await clean(p);
    await p.getByRole("button", { name: "Définitif", exact: true }).click();
    await p.getByRole("button", { name: /^Supprimer définitivement/ }).click();
    await p.getByRole("textbox").fill("SUPPRIM");
  },
});
const run = async (p) => {
  await clean(p);
  await p.getByRole("button", { name: /à la Corbeille/ }).click();
  await p.getByText("Nettoyage terminé").waitFor({ timeout: 10000 });
};
await shot("15-bilan", { act: run });
await shot("16-journal-sombre", {
  theme: "dark",
  act: async (p) => {
    await run(p);
    await p.getByRole("button", { name: "Voir le journal" }).click();
    await p.locator("[aria-expanded]").first().click();
  },
});
// Jalon 6 : nettoyage avancé (dormants, doublons, gros fichiers, gouffres système).
const advanced = async (p, tab) => {
  if (tab === "Gouffres système") {
    // Onglet expérimental : désactivé par défaut (réglage « Expérimental »).
    await p.evaluate(() => {
      const raw = globalThis.localStorage.getItem("lumen-dev-settings");
      const settings = { ...(raw ? JSON.parse(raw) : {}), experimentalSystem: true };
      globalThis.localStorage.setItem("lumen-dev-settings", JSON.stringify(settings));
    });
    await p.reload();
    await p.waitForSelector("#root > *");
  }
  await p.locator("nav").getByRole("button", { name: /^(Nettoyage|Cleanup)$/ }).click();
  if (tab) await p.getByRole("button", { name: tab, exact: true }).click();
};
await shot("17-dormants", {
  act: async (p) => {
    await advanced(p);
    await p.getByRole("checkbox", { name: /Mariage Julie/ }).first().check();
  },
});
await shot("18-doublons", {
  act: async (p) => {
    await advanced(p, "Doublons");
    await p.getByRole("button", { name: "Rechercher les doublons" }).click();
    await p.getByText(/groupes de doublons/).waitFor({ timeout: 10000 });
    await p.getByRole("button", { name: /^Cocher les copies \(/ }).click();
  },
});
await shot("19-gros-fichiers-sombre", {
  theme: "dark",
  act: async (p) => {
    await advanced(p, "Gros fichiers");
    await p.getByRole("button", { name: /^Images disque/ }).click();
  },
});
await shot("20-gouffres", {
  act: async (p) => {
    await advanced(p, "Gouffres système");
    await p.getByRole("checkbox", { name: /WinSxS/ }).check();
    await p.getByRole("group", { name: "Hibernation" }).getByRole("button", { name: "Réduire" }).click();
    await p.getByRole("button", { name: "Lire les points de restauration (autorisation administrateur)" }).click();
    await p.getByRole("checkbox", { name: /Points de restauration/ }).check();
  },
});
await shot("21-confirmation-systeme-sombre", {
  theme: "dark",
  act: async (p) => {
    await advanced(p, "Gouffres système");
    await p.getByRole("checkbox", { name: /WinSxS/ }).check();
    await p.getByRole("group", { name: "Hibernation" }).getByRole("button", { name: "Désactiver" }).click();
    await p.getByRole("button", { name: "Préparer les actions…" }).click();
    await p.getByText("Voir la liste").click();
  },
});
await shot("23-sans-point-de-restauration", {
  act: async (p) => {
    await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-no-protection", "1"));
    await p.reload();
    await p.waitForSelector("#root > *");
    await advanced(p, "Gouffres système");
    await p.getByRole("checkbox", { name: /WinSxS/ }).check();
    await p.getByRole("button", { name: "Préparer les actions…" }).click();
    await p.getByRole("checkbox", { name: "Continuer sans point de restauration" }).check();
  },
});
await shot("22-doublons-anglais", {
  locale: "en-US",
  act: async (p) => {
    await advanced(p, "Duplicates");
    await p.getByRole("button", { name: "Find duplicates" }).click();
    await p.getByText(/duplicate groups/).waitFor({ timeout: 10000 });
  },
});
// Retours de test de la PR #4 : pleine largeur, « Étendre », profondeur adaptative, historique.
for (const [w, h] of [[1280, 800], [1920, 1080], [2560, 1440]]) {
  await shot(`24-pleine-largeur-${w}`, { width: w, height: h, act: (p) => p.waitForSelector("[data-level='3']") });
}
const hoverTile = async (p, name) => {
  const tile = p.locator("[data-node]", { has: p.locator(`:scope > div > span:text-is("${name}")`) }).first();
  const box = await tile.boundingBox();
  if (box) await p.mouse.move(box.x + box.width * 0.6, box.y + 12);
  return tile;
};
await shot("25-etendre-survol", {
  width: 1920,
  height: 1080,
  act: async (p) => {
    const tile = await hoverTile(p, "Captures");
    await tile.locator(":scope > [data-expand]").hover();
    await p.getByRole("tooltip").filter({ hasText: "Étendre : Captures" }).waitFor();
  },
});
await shot("26-historique-apres-etendre", {
  width: 1920,
  height: 1080,
  act: async (p) => {
    const tile = await hoverTile(p, "Captures");
    await tile.locator(":scope > [data-expand]").click();
    await p.getByRole("button", { name: "Précédent : Arthur" }).waitFor();
    await p.getByRole("button", { name: "Précédent : Arthur" }).hover();
  },
});
// Jalon 7 : Timeline (courbe, repères des nettoyages, projection, comparaison), base dans les Paramètres.
const timeline = async (p, range = "3 mois") => {
  await p.getByRole("button", { name: "Timeline", exact: true }).click();
  await p.getByRole("combobox", { name: "Série" }).waitFor();
  await p.getByRole("button", { name: range }).click();
};
const compare = async (p) => {
  await p.getByRole("button", { name: "Comparer", exact: true }).click();
  await p.getByRole("region", { name: "Ont grossi" }).waitFor();
};
await shot("27-timeline-clair", { height: 1300, act: async (p) => {
  await timeline(p);
  await compare(p);
} });
await shot("28-timeline-sombre-1920", { theme: "dark", width: 1920, height: 1300, act: async (p) => {
  await timeline(p, "Tout");
  await compare(p);
  const box = await p.locator("svg[role=img]").boundingBox();
  if (box) await p.mouse.move(box.x + box.width * 0.62, box.y + box.height / 2);
} });
await shot("29-timeline-non-comparable", { act: async (p) => {
  await timeline(p);
  const to = p.getByRole("combobox", { name: "Au" });
  const value = await to.locator("option", { hasText: "MFT" }).first().getAttribute("value");
  if (value) await to.selectOption(value);
  await p.getByRole("button", { name: "Comparer", exact: true }).click();
  await p.getByText(/Lumen n'affiche pas de faux delta/).waitFor();
} });
await shot("30-timeline-cle-usb", { act: async (p) => {
  await timeline(p, "Tout");
  const select = p.getByRole("combobox", { name: "Série" });
  const value = await select.locator("option", { hasText: "Photos" }).first().getAttribute("value");
  if (value) await select.selectOption(value);
  await p.getByText(/Lumen le suit par son identifiant/).waitFor();
} });
await shot("31-parametres-base", { act: async (p) => {
  await p.getByRole("button", { name: "Paramètres" }).click();
  await p.getByRole("button", { name: "Purger…" }).click();
} });
await shot("32-timeline-anglais", { locale: "en-US", height: 1300, act: async (p) => {
  await p.getByRole("button", { name: "Timeline", exact: true }).click();
  await p.getByRole("combobox", { name: "Series" }).waitFor();
  await p.getByRole("button", { name: "3 months" }).click();
  await p.getByRole("button", { name: "Compare", exact: true }).click();
  await p.getByRole("region", { name: "Grew" }).waitFor();
} });
// Jalon 8 : Jeux (launchers, shaders) et Dev (projets, caches, Docker, WSL).
const nav = (p, name) => p.locator("nav").getByRole("button", { name, exact: true }).click();
const experimental = async (p, key) => {
  await p.evaluate((k) => {
    const raw = globalThis.localStorage.getItem("lumen-dev-settings");
    globalThis.localStorage.setItem("lumen-dev-settings", JSON.stringify({ ...(raw ? JSON.parse(raw) : {}), [k]: true }));
  }, key);
  await p.reload();
  await p.waitForSelector("#root > *");
};
await shot("33-jeux", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Jeux");
  await p.getByRole("list", { name: "Jeux installés" }).waitFor();
} });
await shot("34-jeux-sombre-1920", { theme: "dark", width: 1920, height: 1080, act: async (p) => {
  await nav(p, "Jeux");
  await p.getByRole("button", { name: "Dernière partie" }).click();
  await p.getByText(/fichiers de launchers ignorés/).click();
} });
await shot("35-shaders", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Jeux");
  await p.getByRole("button", { name: "Caches de shaders", exact: true }).click();
  await p.getByRole("checkbox", { name: /Inclure les caches de shaders/ }).check();
} });
await shot("36-shaders-jeu-ouvert", { act: async (p) => {
  await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-running", "steam.exe, Cyberpunk2077.exe"));
  await p.reload();
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Jeux");
  await p.getByRole("button", { name: "Caches de shaders", exact: true }).click();
  await p.getByText(/Ouverts maintenant/).waitFor();
} });
await shot("37-dev-projets", { height: 1100, act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Dev");
  await p.getByRole("checkbox", { name: "Cocher les artefacts de hinageshi-web" }).check();
} });
await shot("38-dev-purge-confirmation", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Artefacts", exact: true }).click();
  await p.getByRole("checkbox", { name: "Cocher les artefacts de kaiju-engine" }).check();
  await p.getByRole("checkbox", { name: "Cocher les artefacts de hinageshi-web" }).check();
  await p.getByRole("button", { name: "Purger les artefacts" }).click();
  await p.getByText("Voir la liste").click();
} });
await shot("39-dev-caches-sombre", { theme: "dark", act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Caches globaux" }).click();
} });
await shot("40-dev-docker", { act: async (p) => {
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Docker", exact: true }).click();
  await p.getByText("Supprimer ces images").waitFor();
} });
await shot("41-dev-wsl-desactive", { act: async (p) => {
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Disques WSL" }).click();
} });
await shot("42-dev-wsl", { act: async (p) => {
  await experimental(p, "experimentalWsl");
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Disques WSL" }).click();
  await p.getByRole("checkbox", { name: "Cocher Docker Desktop (données)" }).check();
} });
await shot("43-dev-wsl-confirmation-sombre", { theme: "dark", act: async (p) => {
  await experimental(p, "experimentalWsl");
  await nav(p, "Dev");
  await p.getByRole("button", { name: "Disques WSL" }).click();
  await p.getByRole("checkbox", { name: "Cocher Ubuntu" }).check();
  await p.getByRole("button", { name: "Compacter" }).click();
  await p.getByText("Voir la liste").click();
} });
await shot("44-jeux-anglais", { locale: "en-US", act: async (p) => {
  await nav(p, "Games");
  await p.getByRole("list", { name: "Installed games" }).waitFor();
} });
// Retours de test de la PR #6 : nouveaux launchers, données protégées, mémoire des analyses.
await shot("45-jeux-launchers-1920", { width: 1920, height: 1080, act: async (p) => {
  await nav(p, "Jeux");
  await p.getByRole("list", { name: "Jeux installés" }).waitFor();
  await p.getByRole("listitem", { name: "League of Legends" }).getByRole("button", { name: "Déplacer" }).hover();
} });
await shot("46-parametres-memoire", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await p.getByRole("button", { name: "Paramètres" }).click();
  await p.getByText(/sur 600 Mo/).waitFor();
} });
// Jalon 9 : analyse enregistrée, recherche filtrée, déplacement intelligent, export.
const zoomInto = async (p, name) => {
  // Déplacement intelligent : derrière le réglage Expérimental tant qu'il n'est pas vérifié en VM.
  await experimental(p, "experimentalMove");
  await p.locator("[data-level='3']").first().waitFor();
  const tile = await hoverTile(p, name);
  await tile.locator(":scope > [data-expand]").click();
  await p.getByRole("button", { name: "Déplacer…" }).waitFor();
};
await shot("47-analyse-enregistree", { act: async (p) => {
  await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-restored", "1"));
  await p.reload();
  await p.getByText(/Dernière analyse enregistrée/).waitFor();
} });
await shot("48-palette-filtres", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await p.keyboard.press("Control+K");
  await p.keyboard.type("ext:mp4 >1go cap");
} });
await shot("49-deplacer-verification", { act: async (p) => {
  await zoomInto(p, "Captures");
  await p.getByRole("button", { name: "Déplacer…" }).click();
  await p.getByRole("button", { name: "Copier et vérifier" }).waitFor();
} });
await shot("50-deplacer-refus-sombre", { theme: "dark", act: async (p) => {
  await zoomInto(p, "Docker");
  await p.getByRole("button", { name: "Déplacer…" }).click();
  await p.getByText(/ne peut pas être déplacé/).waitFor();
} });
await shot("51-deplacer-confirmation", { act: async (p) => {
  await zoomInto(p, "Captures");
  await p.getByRole("button", { name: "Déplacer…" }).click();
  await p.getByRole("button", { name: "Copier et vérifier" }).click();
  await p.getByRole("button", { name: /Supprimer l'original/ }).waitFor({ timeout: 10_000 });
  await p.getByLabel("Je confirme la suppression définitive de l'original").check();
} });
await shot("52-jonction-absente", { act: async (p) => {
  await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-moved", "1"));
  await p.reload();
  await p.getByText("Un dossier déplacé par Lumen est inaccessible").waitFor();
} });
await shot("53-journal-deplacements-sombre", { theme: "dark", act: async (p) => {
  await p.evaluate(() => globalThis.localStorage.setItem("lumen-dev-moved", "1"));
  await p.reload();
  await nav(p, "Journal");
  await p.getByRole("heading", { name: "Déplacements" }).waitFor();
} });
await shot("54-export", { act: async (p) => {
  await p.locator("[data-level='3']").first().waitFor();
  await p.getByRole("button", { name: "Exporter", exact: true }).click();
} });
// Le rapport lui-même : téléchargé depuis le coffre factice puis ouvert seul, hors ligne.
for (const [name, theme] of [["55-rapport-html", "light"], ["56-rapport-html-sombre", "dark"]]) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1024 }, locale: "fr-FR", colorScheme: theme });
  await welcomed(page);
  await page.goto(url);
  await page.locator("[data-level='3']").first().waitFor();
  await page.getByRole("button", { name: "Exporter", exact: true }).click();
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("dialog").getByRole("button", { name: "Exporter", exact: true }).click()]);
  const file = join(out, `${name}.html`);
  await download.saveAs(file);
  const report = await browser.newPage({ viewport: { width: 1200, height: 1600 }, offline: true });
  await report.route("**/*", (r) => (r.request().url().startsWith("file:") ? r.continue() : r.abort()));
  await report.goto(`file://${join(process.cwd(), file)}`);
  await report.evaluate(() => globalThis.document.fonts.ready);
  await report.screenshot({ path: join(out, `${name}.png`), fullPage: true });
  await report.close();
  await page.close();
}
// Jalon 10 : tutoriel de bienvenue (4 écrans) et astuces contextuelles.
const nextScreen = (p, n) => Array.from({ length: n }).reduce((acc) => acc.then(() => p.getByRole("button", { name: /^(Suivant|Next)$/ }).click()), Promise.resolve());
for (const [i, theme] of [[1, "light"], [2, "light"], [3, "dark"], [4, "light"]]) {
  await shot(`57-bienvenue-${i}${theme === "dark" ? "-sombre" : ""}`, { theme, fresh: { onboarding: false }, act: async (p) => {
    await p.getByRole("dialog").waitFor();
    await nextScreen(p, i - 1);
    await p.waitForTimeout(700);
  } });
}
// Mouvement réduit : pas d'animation, l'état final de chaque écran tout de suite.
await shot("57-bienvenue-3-mouvement-reduit", { reducedMotion: "reduce", fresh: { onboarding: false }, act: async (p) => {
  await p.getByRole("dialog").waitFor();
  await nextScreen(p, 2);
} });
await shot("58-bienvenue-anglais", { locale: "en-US", fresh: { onboarding: false }, act: async (p) => {
  await p.getByRole("dialog").waitFor();
  await nextScreen(p, 1);
  await p.waitForTimeout(700);
} });
await shot("59-astuce-analyse", { fresh: { tips: [] }, act: (p) => p.getByRole("note").waitFor() });
await shot("60-astuce-timeline-sombre", { theme: "dark", fresh: { tips: ["analyse"] }, act: async (p) => {
  await nav(p, "Timeline");
  await p.getByRole("note").waitFor();
} });
await shot("61-astuce-journal", { fresh: { tips: ["analyse"] }, act: async (p) => {
  await nav(p, "Journal");
  await p.getByRole("note").waitFor();
} });
await shot("62-parametres-aide", { act: async (p) => {
  await p.getByRole("button", { name: "Paramètres" }).click();
  await p.getByRole("button", { name: "Réinitialiser les astuces" }).scrollIntoViewIfNeeded();
} });
// Jalon 11 : intégration Windows (Paramètres, À propos, « Analyser avec Lumen »).
const settingsAt = (label) => async (p) => {
  await p.getByRole("button", { name: /^(Paramètres|Settings)$/ }).click();
  await p.getByRole("region", { name: label }).scrollIntoViewIfNeeded();
};
await shot("63-parametres-windows", { act: settingsAt("À propos") });
await shot("64-parametres-windows-sombre", { theme: "dark", act: settingsAt("À propos") });
await shot("65-licences-tierces", { act: async (p) => {
  await settingsAt("À propos")(p);
  await p.getByRole("button", { name: "Afficher" }).click();
  await p.getByRole("dialog").getByText("Lumen — composants tiers").waitFor();
} });
await shot("66-parametres-anglais", { locale: "en-US", act: settingsAt("About") });
// Comme depuis l'Explorateur : `lumen.exe --lumen-open "<dossier>"` (coffre factice : lumen-dev-open).
const opened = (path) => async (p) => {
  await p.evaluate((v) => globalThis.localStorage.setItem("lumen-dev-open", v), path);
  await p.reload();
  await p.waitForSelector("#root > *");
};
await shot("67-analyser-avec-lumen", { act: async (p) => {
  await opened("C:\\Users\\Arthur\\Vidéos")(p);
  await p.locator("[data-level='2']").first().waitFor();
  await p.waitForTimeout(600);
} });
await shot("68-analyser-avec-lumen-refus", { act: async (p) => {
  await opened("\\\\NAS\\partage\\Films")(p);
  await p.getByText("Impossible d'ouvrir ce dossier").waitFor();
} });
await browser.close();
console.log(`captures dans ${out}`);
