// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetDepth, useDepth } from "../../app/depth";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import { KpiBand } from "../kpi/KpiBand";
import { DepthScreen } from "./DepthScreen";
import { App } from "../../app/App";

describe("Profondeur", () => {
  beforeEach(() => {
    resetDepth();
    useStore.setState({
      locale: "fr",
      screen: "depth",
      settings: { ...DEFAULT_SETTINGS },
      appInfo: { elevated: false, fastScanAvailable: true, version: "0.9.0", profile: "C:\\Users\\arthur" },
      clean: CLEAN_CLOSED,
      cleanupTab: "dormant",
    });
  });

  afterEach(() => vi.restoreAllMocks());

  it("analyse le volume en lecture seule : postes, renvois vers les modules, aucune case à cocher", async () => {
    const start = vi.spyOn(backend, "startScan");
    // Aucune analyse du volume gardée : l'écran propose de la lancer.
    vi.spyOn(backend, "cachedScan").mockResolvedValue(null);
    render(<DepthScreen />);
    fireEvent.click(await screen.findByRole("button", { name: "Analyser en profondeur" }));
    expect(start).toHaveBeenCalledWith({ kind: "volume", letter: "C:" }, "auto");

    const items = await screen.findByRole("region", { name: "Postes du volume" }, { timeout: 2000 });
    for (const name of ["Windows", "Applications", "Fichiers système", "Points de restauration", "Autres comptes", "Microsoft Store et Xbox", "Inaccessible"]) {
      expect(within(items).getByText(name)).toBeTruthy();
    }
    // Lecture seule : ni case à cocher, ni bouton de nettoyage.
    expect(screen.queryAllByRole("checkbox")).toHaveLength(0);
    expect(screen.queryByRole("button", { name: /Nettoyer|Supprimer|Corbeille/ })).toBeNull();
    // Autre compte : taille seulement, jamais un lien vers son contenu.
    const profiles = items.querySelector('[data-item="profiles"]') as HTMLElement;
    expect(within(profiles).getByText("Admin")).toBeTruthy();
    expect(within(profiles).queryByRole("button", { name: /Admin/ })).toBeNull();
    // Zones de la denylist « Protégées » dans la treemap, jamais hachurées.
    expect(document.querySelectorAll("[data-protected]").length).toBeGreaterThan(0);
    expect(document.querySelectorAll("[data-hatch]")).toHaveLength(0);

    // Applications : la désinstallation passe par les Paramètres de Windows.
    const open = vi.spyOn(backend, "openAppsSettings");
    fireEvent.click(within(items.querySelector('[data-item="programFiles"]') as HTMLElement).getByRole("button", { name: "Désinstaller via Paramètres Windows" }));
    expect(open).toHaveBeenCalledOnce();
    // Windows : renvoi vers les Gouffres système.
    fireEvent.click(within(items.querySelector('[data-item="windows"]') as HTMLElement).getByRole("button", { name: /Gouffres système/ }));
    expect(useStore.getState().screen).toBe("cleanup");
    expect(useStore.getState().cleanupTab).toBe("system");
  });

  it("clichés instantanés lus sur demande, zoom dans la treemap, KPI « Analysé en profondeur il y a X »", async () => {
    const id = await backend.startScan({ kind: "volume", letter: "C:" }, "auto");
    await new Promise<void>((resolve) => {
      const off = backend.onScanEvent((e) => {
        if (e.type === "done" && e.summary.scanId === id) {
          off();
          resolve();
        }
      });
    });
    render(
      <>
        <KpiBand />
        <DepthScreen />
      </>,
    );
    // L'analyse du volume déjà gardée est reprise sans réanalyser.
    await screen.findByRole("region", { name: "Postes du volume" });
    expect(useDepth.getState().scanId).toBe(id);
    expect(screen.getByText(/^Analysé en profondeur il y a/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Lire l'espace des clichés (admin)" }));
    expect(await screen.findByText("7,4 Go utilisés · 23,8 Go au plus")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "WinSxS" }));
    await waitFor(() => expect(screen.getByRole("navigation", { name: "Fil d'Ariane" }).textContent).toContain("WinSxS"));
  });

  it("treemap du volume : précédent / suivant, fil d'Ariane, Étendre et raccourcis, comme l'Explorer", async () => {
    const id = await backend.startScan({ kind: "volume", letter: "C:" }, "auto");
    await new Promise<void>((resolve) => {
      const off = backend.onScanEvent((e) => {
        if (e.type === "done" && e.summary.scanId === id) {
          off();
          resolve();
        }
      });
    });
    render(<App />);
    act(() => useStore.getState().go("depth"));
    await screen.findByRole("region", { name: "Postes du volume" });
    const crumbs = () => screen.getByRole("navigation", { name: "Fil d'Ariane" }).textContent ?? "";
    const back = () => screen.getByRole("button", { name: /^Précédent/ }) as HTMLButtonElement;
    const forward = () => screen.getByRole("button", { name: /^Suivant/ }) as HTMLButtonElement;
    expect(back().disabled).toBe(true);
    // Zoom par une tuile (clic), puis Étendre sur une sous-tuile.
    fireEvent.click(screen.getByRole("button", { name: /^Windows,/ }));
    await waitFor(() => expect(crumbs()).toContain("Windows"));
    expect(back().getAttribute("aria-label")).toBe("Précédent : C:");
    const winsxs = document.querySelector('[data-node] [data-expand]') as HTMLElement;
    fireEvent.click(winsxs);
    await waitFor(() => expect(useDepth.getState().back).toHaveLength(2));
    // ← (bouton), puis Alt+→ (clavier) et Retour arrière, puis bouton 4 de la souris.
    fireEvent.click(back());
    await waitFor(() => expect(crumbs()).toMatch(/Windows$/));
    expect(forward().disabled).toBe(false);
    fireEvent.keyDown(window, { key: "ArrowRight", altKey: true });
    await waitFor(() => expect(useDepth.getState().back).toHaveLength(2));
    fireEvent.keyDown(window, { key: "Backspace" });
    await waitFor(() => expect(crumbs()).toMatch(/Windows$/));
    fireEvent.mouseUp(window, { button: 3 });
    await waitFor(() => expect(crumbs()).toBe("C:"));
    // Fil d'Ariane cliquable : nouvel emplacement dans l'historique.
    fireEvent.click(screen.getByRole("button", { name: /^Program Files,/ }));
    await waitFor(() => expect(crumbs()).toContain("Program Files"));
    fireEvent.click(within(screen.getByRole("navigation", { name: "Fil d'Ariane" })).getByRole("button", { name: "C:" }));
    await waitFor(() => expect(crumbs()).toBe("C:"));
    expect(back().getAttribute("aria-label")).toBe("Précédent : Program Files");
  });
});
