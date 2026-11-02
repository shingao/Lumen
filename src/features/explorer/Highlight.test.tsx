// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { goBack, goTo, resetNavigation } from "../../app/navigation";
import { startScan } from "../../app/scanning";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import { AnalyseScreen } from "../screens/AnalyseScreen";

const dimmed = () => document.querySelectorAll("[data-dim]").length;
const chip = () => document.querySelector<HTMLElement>('[role="status"][class*="chip"]');
const tile = (name: RegExp) => screen.getByRole("button", { name });

async function idOf(path: string): Promise<number> {
  const id = await backend.findNode(useStore.getState().scan.summary?.scanId ?? 0, path);
  if (id === null) throw new Error(path);
  return id;
}

describe("Estompage de la treemap : trois sources, arrêt net", () => {
  beforeEach(async () => {
    resetNavigation();
    useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS }, clean: CLEAN_CLOSED, node: null, target: { kind: "userProfile" }, openReco: null, hoverReco: null, statusFocus: null, screen: "analyse" });
    await act(() => startScan({ kind: "userProfile" }));
    await waitFor(() => expect(useStore.getState().scan.status).toBe("done"));
  });

  it("panneau ouvert par une tuile : estompage et puce ✕ ; ✕ ferme le panneau et tout redevient net", async () => {
    render(<AnalyseScreen />);
    fireEvent.click(await screen.findByRole("button", { name: /^Contenu interne du cache npm,/ }));
    expect(useStore.getState().openReco).toBe("npm");
    expect(dimmed()).toBeGreaterThan(0);
    expect(tile(/^Vidéos,/).dataset.dim).toBeDefined();
    const status = chip() as HTMLElement;
    expect(status.textContent).toBe("Mise en évidence : Cache npm");
    fireEvent.click(within(status).getByRole("button", { name: "Arrêter la mise en évidence (Cache npm)" }));
    expect(useStore.getState().openReco).toBeNull();
    expect(dimmed()).toBe(0);
    expect(chip()).toBeNull();
  });

  it("zoom, fil d'Ariane, ← →, autre tuile, onglet : la mise en évidence du panneau s'arrête", async () => {
    render(<AnalyseScreen />);
    const npm = await screen.findByRole("button", { name: /^Contenu interne du cache npm,/ });
    // Zoom ailleurs (le bug : tout restait estompé dans un dossier sans tuile pip).
    fireEvent.click(npm);
    act(() => goTo(null));
    const videos = await idOf("C:\\Users\\Arthur\\Vidéos");
    act(() => goTo(videos));
    expect(useStore.getState().openReco).toBeNull();
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Captures,/ })).toBeTruthy());
    expect(dimmed()).toBe(0);
    // ← (navigation) : idem.
    act(() => useStore.getState().setOpenReco("npm"));
    act(() => goBack());
    expect(useStore.getState().openReco).toBeNull();
    // Autre tuile (dossier) : zoom, panneau fermé.
    await screen.findByRole("button", { name: /^Contenu interne du cache npm,/ });
    act(() => useStore.getState().setOpenReco("npm"));
    fireEvent.click(tile(/^Vidéos,/));
    expect(useStore.getState().openReco).toBeNull();
    // Onglet : idem.
    act(() => useStore.getState().setOpenReco("npm"));
    act(() => useStore.getState().go("timeline"));
    expect(useStore.getState().openReco).toBeNull();
    // Volume : idem.
    act(() => useStore.getState().setOpenReco("npm"));
    act(() => useStore.getState().setTarget({ kind: "volume", letter: "D:" }));
    expect(useStore.getState().openReco).toBeNull();
  });

  it("« ? » d'un statut : persiste au zoom, puce « aucun élément ici » sans estompage, ✕ arrête", async () => {
    render(<AnalyseScreen />);
    await screen.findByRole("button", { name: /^Contenu interne du cache npm,/ });
    fireEvent.click(screen.getAllByRole("button", { name: "Montrer dans la treemap ce qui est sans risque" })[0] as HTMLElement);
    expect(dimmed()).toBeGreaterThan(0);
    expect((chip() as HTMLElement).textContent).toBe("Mise en évidence : Sans risque");
    // Dans Vidéos : aucune tuile sans risque, rien d'estompé, la puce le dit.
    const videos = await idOf("C:\\Users\\Arthur\\Vidéos");
    act(() => goTo(videos));
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Captures,/ })).toBeTruthy());
    expect(useStore.getState().statusFocus?.status).toBe("safe");
    expect(dimmed()).toBe(0);
    const absent = chip() as HTMLElement;
    expect(absent.textContent).toMatch(/^Mise en évidence : Sans risque/);
    expect(absent.textContent).toContain("aucun élément ici");
    fireEvent.click(within(chip() as HTMLElement).getByRole("button"));
    expect(useStore.getState().statusFocus).toBeNull();
    expect(chip()).toBeNull();
  });

  it("une seule source : le panneau arrête le « ? », le « ? » ferme le panneau ; le survol est passager", async () => {
    render(<AnalyseScreen />);
    await screen.findByRole("button", { name: /^Contenu interne du cache npm,/ });
    act(() => useStore.getState().setStatusFocus({ status: "review", from: "kpi" }));
    act(() => useStore.getState().setOpenReco("npm"));
    expect(useStore.getState().statusFocus).toBeNull();
    act(() => useStore.getState().setStatusFocus({ status: "safe", from: "kpi" }));
    expect(useStore.getState().openReco).toBeNull();
    act(() => useStore.getState().setStatusFocus(null));
    // Survol d'une recommandation : estompage pendant le survol seulement, sans puce.
    act(() => useStore.getState().setHoverReco("npm"));
    expect(dimmed()).toBeGreaterThan(0);
    expect(chip()).toBeNull();
    act(() => useStore.getState().setHoverReco(null));
    expect(dimmed()).toBe(0);
    // Survol d'une recommandation hors de la vue : rien d'estompé.
    const videos = await idOf("C:\\Users\\Arthur\\Vidéos");
    act(() => goTo(videos));
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Captures,/ })).toBeTruthy());
    act(() => useStore.getState().setHoverReco("npm"));
    expect(dimmed()).toBe(0);
  });
});
