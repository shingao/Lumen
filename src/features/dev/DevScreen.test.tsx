// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { resetDevGames, useDevGames } from "../../app/devgames";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { ScanSummary } from "../../ipc/types";
import { CleanDialog } from "../cleanup/CleanDialog";
import { DevScreen } from "./DevScreen";

const text = (el: Element | null) => (el?.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");

async function scanned(experimentalWsl = false) {
  const scanId = await backend.startScan({ kind: "userProfile" }, "auto");
  const summary = await new Promise<ScanSummary>((resolve) => {
    const off = backend.onScanEvent((e) => {
      if (e.type === "done" && e.summary.scanId === scanId) {
        off();
        resolve(e.summary);
      }
    });
  });
  useStore.setState({
    locale: "fr",
    settings: { ...DEFAULT_SETTINGS, experimentalWsl },
    scan: { id: scanId, status: "done", method: "auto", progress: null, summary, message: null },
    clean: CLEAN_CLOSED,
  });
}

const tab = (name: string) => fireEvent.click(screen.getByRole("button", { name }));
const rowNames = () =>
  within(screen.getByRole("region", { name: "Projets" }))
    .getAllByRole("listitem")
    .map((li) => li.getAttribute("aria-label"))
    .filter(Boolean);

describe("Dev", () => {
  beforeEach(async () => {
    resetDevGames();
    useDevGames.setState({ scanId: null });
    await scanned();
  });

  afterEach(() => localStorage.removeItem("lumen-dev-docker"));

  it("projets : triés par inactivité ou par artefacts, rien de coché, purge par identifiants", async () => {
    render(
      <>
        <DevScreen />
        <CleanDialog />
      </>,
    );
    await screen.findByRole("region", { name: "Projets" });
    expect(rowNames()[0]).toBe("kaiju-engine");
    tab("Artefacts");
    expect(rowNames().slice(0, 2)).toEqual(["kaiju-engine", "notebooks-meteo"]);
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.every((b) => !b.checked)).toBe(true);
    // Sans lockfile ni artefact : case désactivée et explication.
    expect((screen.getByRole("checkbox", { name: "Cocher les artefacts de prototype-canvas" }) as HTMLInputElement).disabled).toBe(true);
    expect(screen.getAllByText(/node_modules ne se régénère pas à l'identique/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("checkbox", { name: "Cocher les artefacts de hinageshi-web" }));
    expect(text(screen.getByText(/éléments cochés/))).toMatch(/^2 éléments cochés · 1,5 Go$/);
    fireEvent.click(screen.getByRole("button", { name: "Purger les artefacts" }));
    expect(useStore.getState().clean.request).toEqual({ kind: "artifacts", ids: [9101, 9102] });
    expect(await screen.findByText("C:\\Users\\Arthur\\Projets\\hinageshi-web\\node_modules\\")).toBeTruthy();
  });

  it("caches globaux : Cargo et .m2 « À vérifier », commande officielle signalée", async () => {
    render(<DevScreen />);
    tab("Caches globaux");
    const list = await screen.findByRole("list", { name: "Caches globaux" });
    const cargo = within(list).getByText("Registre Cargo").closest("li") as HTMLElement;
    expect(within(cargo).getByText("À vérifier")).toBeTruthy();
    const m2 = within(list).getByText("Dépôt Maven local (.m2)").closest("li") as HTMLElement;
    expect(within(m2).getByText("À vérifier")).toBeTruthy();
    const npm = within(list).getByText("Cache npm").closest("li") as HTMLElement;
    expect(within(npm).getByText("commande officielle")).toBeTruthy();
    expect((within(list).getAllByRole("checkbox") as HTMLInputElement[]).every((b) => !b.checked)).toBe(true);
  });

  it("Docker : liste des images, rien sans Docker lancé", async () => {
    render(<DevScreen />);
    tab("Docker");
    expect(await screen.findAllByText("3 images sans étiquette")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Supprimer ces images" }));
    expect(useStore.getState().clean.request).toEqual({ kind: "dockerPrune" });
  });

  it("Docker arrêté : pas de prune possible", async () => {
    localStorage.setItem("lumen-dev-docker", "off");
    render(<DevScreen />);
    tab("Docker");
    expect(await screen.findByText(/Docker ne tourne pas : lance Docker Desktop/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Supprimer ces images" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("WSL : derrière le réglage Expérimental, avertissement wsl --shutdown, jamais coché", async () => {
    expect(DEFAULT_SETTINGS.experimentalWsl).toBe(false);
    const { unmount } = render(<DevScreen />);
    tab("Disques WSL");
    expect(screen.getByText(/Le compactage des disques WSL \/ Docker est expérimental/)).toBeTruthy();
    unmount();
    await scanned(true);
    render(
      <>
        <DevScreen />
        <CleanDialog />
      </>,
    );
    tab("Disques WSL");
    expect(screen.getByText(/arrête tous les processus Linux et Docker en cours/)).toBeTruthy();
    expect(screen.getByText(/attaché en lecture seule/)).toBeTruthy();
    const list = await screen.findByRole("list", { name: "Disques WSL" });
    const boxes = within(list).getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes).toHaveLength(3);
    expect(boxes.every((b) => !b.checked)).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: "Cocher Ubuntu" }));
    fireEvent.click(screen.getByRole("button", { name: "Compacter" }));
    expect(useStore.getState().clean.request).toEqual({ kind: "wslCompact", ids: [0] });
    expect(await screen.findByText("wsl.exe --shutdown")).toBeTruthy();
    expect(screen.getByText(/compactés, jamais supprimés/)).toBeTruthy();
  });
});
