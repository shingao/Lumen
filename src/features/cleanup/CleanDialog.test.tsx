// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { openCleanup, permanentPhraseOk } from "../../app/cleaning";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { ScanSummary } from "../../ipc/types";
import { JournalScreen } from "../journal/JournalScreen";
import { CleanDialog } from "./CleanDialog";

const text = (el: Element | null) => (el?.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");

async function scanned(permanentDelete = false) {
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
    settings: { ...DEFAULT_SETTINGS, permanentDelete },
    scan: { id: scanId, status: "done", method: "auto", progress: null, summary, message: null },
    clean: CLEAN_CLOSED,
  });
}

describe("Nettoyage", () => {
  beforeEach(() => scanned());

  it("la confirmation montre total, nombre, méthode, disque et le bouton porte le total", async () => {
    render(<CleanDialog />);
    act(() => openCleanup(["temp", "gradle"]));
    const go = await screen.findByRole("button", { name: /à la Corbeille/ });
    expect(text(go)).toMatch(/^Mettre \d+,\d Go à la Corbeille$/);
    const dialog = screen.getByRole("dialog");
    expect(text(dialog)).toContain("Corbeille");
    expect(text(dialog)).toMatch(/C: · [\d,]+ Go libres/);
    expect(text(dialog)).toContain("Voir la liste");
    // Rien n'est fait tant que ce n'est pas confirmé.
    expect(await backend.journal()).toHaveLength(0);
  });

  it("exécution : avancement puis bilan, entrée au journal, restauration", async () => {
    render(<CleanDialog />);
    act(() => openCleanup(["temp"]));
    fireEvent.click(await screen.findByRole("button", { name: /à la Corbeille/ }));
    expect(await screen.findByText("Nettoyage en cours")).toBeTruthy();
    expect(await screen.findByText("Nettoyage terminé", undefined, { timeout: 3000 })).toBeTruthy();
    expect(text(screen.getByRole("dialog"))).toContain("Libéré");
    fireEvent.click(screen.getByRole("button", { name: "Voir le journal" }));
    expect(useStore.getState().screen).toBe("journal");

    render(<JournalScreen />);
    fireEvent.click(await screen.findByRole("button", { expanded: false, name: /Fichiers temporaires/ }));
    const restore = await screen.findAllByRole("button", { name: "Restaurer depuis la Corbeille" });
    fireEvent.click(restore[0] as HTMLElement);
    await waitFor(() => expect(screen.getAllByText("restauré").length).toBe(1));
  });

  it("définitif : seconde confirmation où il faut taper le total ou SUPPRIMER", async () => {
    await scanned(true);
    render(<CleanDialog />);
    act(() => openCleanup(["gradle"]));
    fireEvent.click(await screen.findByRole("button", { name: "Définitif" }));
    const go = await screen.findByRole("button", { name: /^Supprimer définitivement/ });
    fireEvent.click(go);
    expect(await screen.findByText("Seconde confirmation")).toBeTruthy();
    const confirm = screen.getByRole("button", { name: /^Supprimer définitivement/ }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "oui" } });
    expect(confirm.disabled).toBe(true);
    fireEvent.change(input, { target: { value: "supprimer" } });
    expect(confirm.disabled).toBe(false);
    // Retour : on revient à la première confirmation, rien n'est fait.
    fireEvent.click(screen.getByRole("button", { name: "Retour" }));
    expect(await screen.findByRole("button", { name: /^Supprimer définitivement/ })).toBeTruthy();
  });

  it("le définitif n'est proposé que si les réglages l'autorisent", async () => {
    render(<CleanDialog />);
    act(() => openCleanup(["gradle"]));
    await screen.findByRole("button", { name: /à la Corbeille/ });
    expect(screen.queryByRole("button", { name: "Définitif" })).toBeNull();
  });

  it("saisie du définitif : total (espaces indifférents) ou SUPPRIMER / DELETE", () => {
    expect(permanentPhraseOk("44,2 Go", "44,2 Go")).toBe(true);
    expect(permanentPhraseOk(" 44,2go ", "44,2 Go")).toBe(true);
    expect(permanentPhraseOk("SUPPRIMER", "1 Go")).toBe(true);
    expect(permanentPhraseOk("delete", "1 GB")).toBe(true);
    expect(permanentPhraseOk("44 Go", "44,2 Go")).toBe(false);
    expect(permanentPhraseOk("", "")).toBe(false);
  });
});
