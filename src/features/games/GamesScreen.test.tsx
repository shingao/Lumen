// @vitest-environment jsdom
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDevGames } from "../../app/devgames";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { ScanSummary } from "../../ipc/types";
import { CleanDialog } from "../cleanup/CleanDialog";
import { GamesScreen } from "./GamesScreen";

async function scanned() {
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
    settings: { ...DEFAULT_SETTINGS },
    scan: { id: scanId, status: "done", method: "auto", progress: null, summary, message: null },
    clean: CLEAN_CLOSED,
  });
}

const tab = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("Jeux", () => {
  beforeEach(async () => {
    useDevGames.setState({ games: { status: "idle" }, covers: {}, running: [], shaders: { status: "idle" }, shadersOn: false, scanId: null, openError: null });
    await scanned();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.removeItem("lumen-dev-running");
  });

  it("liste les jeux : taille, disque, dernière partie connue ou « inconnue », statut, aucune case", async () => {
    render(<GamesScreen />);
    const list = await screen.findByRole("list", { name: "Jeux installés" });
    const cards = within(list).getAllByRole("listitem");
    expect(cards.length).toBe(7);
    // Trié par taille : Baldur's Gate 3 d'abord.
    expect(cards[0]?.getAttribute("aria-label")).toBe("Baldur's Gate 3");
    expect(within(list).queryAllByRole("checkbox")).toHaveLength(0);
    const cyberpunk = within(list).getByRole("listitem", { name: "Cyberpunk 2077" });
    expect(within(cyberpunk).getByText("À vérifier")).toBeTruthy();
    expect(within(cyberpunk).getByText(/Steam · D:/)).toBeTruthy();
    const fortnite = within(list).getByRole("listitem", { name: "Fortnite" });
    expect(within(fortnite).getByText("Dernière partie inconnue")).toBeTruthy();
    expect(within(fortnite).getByText("À garder")).toBeTruthy();
    const witcher = within(list).getByRole("listitem", { name: "The Witcher 3: Wild Hunt" });
    expect(within(witcher).getByText("mesurée")).toBeTruthy();
    expect(screen.getByText(/2 fichiers de launchers ignorés/)).toBeTruthy();
  });

  it("Désinstaller et Déplacer ouvrent le launcher, jamais une suppression", async () => {
    const open = vi.spyOn(backend, "openGame");
    const plan = vi.spyOn(backend, "planCleanup");
    render(<GamesScreen />);
    const card = await screen.findByRole("listitem", { name: "Counter-Strike 2" });
    fireEvent.click(within(card).getByRole("button", { name: "Désinstaller" }));
    fireEvent.click(within(card).getByRole("button", { name: "Déplacer" }));
    expect(open.mock.calls).toEqual([
      ["steam:730", "uninstall"],
      ["steam:730", "move"],
    ]);
    expect(plan).not.toHaveBeenCalled();
  });

  it("tri par dernière partie : la plus ancienne d'abord, inconnue en dernier", async () => {
    render(<GamesScreen />);
    await screen.findByRole("list", { name: "Jeux installés" });
    tab("Dernière partie");
    const names = within(screen.getByRole("list", { name: "Jeux installés" }))
      .getAllByRole("listitem")
      .map((c) => c.getAttribute("aria-label"));
    expect(names.slice(0, 2)).toEqual(["Elden Ring", "Cyberpunk 2077"]);
    expect(names.slice(-3)).toEqual(expect.arrayContaining(["Fortnite", "Diablo IV"]));
  });

  it("shaders : À vérifier, jamais cochés, explication et conseil, shadercache Steam informatif", async () => {
    render(
      <>
        <GamesScreen />
        <CleanDialog />
      </>,
    );
    tab("Caches de shaders");
    const include = (await screen.findByRole("checkbox", { name: /Inclure les caches de shaders/ })) as HTMLInputElement;
    expect(include.checked).toBe(false);
    expect(screen.getByText(/recompilent leurs shaders/)).toBeTruthy();
    expect(screen.getByText(/Panneau de configuration NVIDIA/)).toBeTruthy();
    expect(screen.getByText(/Lumen n'y touche pas/)).toBeTruthy();
    const clean = screen.getByRole("button", { name: "Nettoyer la sélection…" }) as HTMLButtonElement;
    expect(clean.disabled).toBe(true);
    fireEvent.click(include);
    expect(clean.disabled).toBe(false);
    fireEvent.click(clean);
    expect(useStore.getState().clean.request).toEqual({ kind: "shaders" });
    expect(await screen.findByText(/Refusé au moment de l'action si un jeu ou un launcher est ouvert/)).toBeTruthy();
  });

  it("shaders : un jeu ou un launcher ouvert bloque le nettoyage", async () => {
    localStorage.setItem("lumen-dev-running", "steam.exe");
    render(<GamesScreen />);
    tab("Caches de shaders");
    expect(await screen.findByText(/Ouverts maintenant : steam.exe/)).toBeTruthy();
    const include = (await screen.findByRole("checkbox", { name: /Inclure les caches de shaders/ })) as HTMLInputElement;
    expect(include.disabled).toBe(true);
    act(() => useDevGames.setState({ shadersOn: true }));
    expect((screen.getByRole("button", { name: "Nettoyer la sélection…" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
