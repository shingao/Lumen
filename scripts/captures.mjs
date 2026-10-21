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

async function shot(name, { theme = "light", locale = "fr-FR", width = 1440, height = 1024, act } = {}) {
  const page = await browser.newPage({ viewport: { width, height }, locale, colorScheme: theme });
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
await browser.close();
console.log(`captures dans ${out}`);
