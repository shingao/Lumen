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
await browser.close();
console.log(`captures dans ${out}`);
