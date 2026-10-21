// @vitest-environment jsdom
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { setRestorePoint, useAdvanced } from "../../app/advanced";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { ScanSummary } from "../../ipc/types";
import { CleanDialog } from "../cleanup/CleanDialog";
import { CleanupScreen } from "./CleanupScreen";

const text = (el: Element | null) => (el?.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");

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

describe("Nettoyage avancé", () => {
  beforeEach(async () => {
    useAdvanced.setState({ systemActions: [], restorePoint: true, restorePoints: { status: "idle" }, sinks: { status: "idle" } });
    await scanned();
  });

  it("dormants : base de date affichée, rien de coché par défaut, sélection par identifiants", async () => {
    render(
      <>
        <CleanupScreen />
        <CleanDialog />
      </>,
    );
    expect(await screen.findByText(/fsutil behavior query disablelastaccess = 0x80000002/)).toBeTruthy();
    const boxes = await screen.findAllByRole("checkbox");
    expect(boxes.every((b) => !(b as HTMLInputElement).checked)).toBe(true);
    const clean = screen.getByRole("button", { name: "Nettoyer la sélection…" });
    expect((clean as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: /Cocher tout le dossier ~\\Videos\\Mariage Julie/ }));
    expect(text(screen.getByText(/fichiers cochés/))).toMatch(/^3 fichiers cochés · 35,1 Go$/);
    fireEvent.click(clean);
    expect(useStore.getState().clean.request).toMatchObject({ kind: "dormant" });
    const go = await screen.findByRole("button", { name: /à la Corbeille/ });
    expect(text(go)).toBe("Mettre 35,1 Go à la Corbeille");
  });

  it("doublons : jamais tous les exemplaires d'un groupe", async () => {
    render(<CleanupScreen />);
    tab("Doublons");
    fireEvent.click(screen.getByRole("button", { name: "Rechercher les doublons" }));
    const group = await screen.findByRole("region", { name: /^3 exemplaires de 9,1 Go$/ }, { timeout: 3000 });
    const boxes = within(group).getAllByRole("checkbox") as HTMLInputElement[];
    act(() => {
      fireEvent.click(boxes[0] as HTMLInputElement);
      fireEvent.click(boxes[1] as HTMLInputElement);
    });
    // Deux cochés sur trois : le dernier ne peut plus l'être.
    expect((boxes[2] as HTMLInputElement).disabled).toBe(true);
    expect(useAdvanced.getState().checkedDup.size).toBe(2);
    fireEvent.click(within(group).getByRole("button", { name: "Cocher les copies" }));
    expect(boxes.map((b) => b.checked)).toEqual([false, true, true]);
    expect(within(group).getByText("gardé")).toBeTruthy();
  });

  it("gouffres système : commandes élevées après le point de restauration obligatoire", async () => {
    render(
      <>
        <CleanupScreen />
        <CleanDialog />
      </>,
    );
    tab("Gouffres système");
    fireEvent.click(await screen.findByRole("checkbox", { name: /WinSxS/ }));
    fireEvent.click(screen.getByRole("button", { name: "Préparer les actions…" }));
    const go = await screen.findByRole("button", { name: "Lancer 1 action système" });
    const dialog = screen.getByRole("dialog");
    expect(text(dialog)).toContain("Prérequis obligatoire");
    expect(text(dialog)).toContain("Chaque commande système demande ton autorisation (UAC)");
    expect(text(dialog)).toContain("/StartComponentCleanup");
    expect(text(dialog)).not.toContain("/ResetBase");
    fireEvent.click(go);
    expect(await screen.findByText("déjà fait", undefined, { timeout: 3000 })).toBeTruthy();
    const journal = await backend.journal();
    expect(journal[0]?.kind).toBe("system");
  });

  it("plus ancien point de restauration : jamais sans lire la liste, date du point affichée", async () => {
    render(<CleanupScreen />);
    tab("Gouffres système");
    const include = (await screen.findByRole("checkbox", { name: /Points de restauration/ })) as HTMLInputElement;
    expect(include.disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Lire les points de restauration (autorisation administrateur)" }));
    const oldest = await screen.findByText(/^Point qui serait supprimé : /);
    expect(text(oldest)).toMatch(/^Point qui serait supprimé : \d\d\/\d\d\/\d{4} \(il y a \d+ mois\) — Windows Update$/);
    expect(include.disabled).toBe(false);
    expect(text(screen.getByRole("region", { name: "Points de restauration" }))).toContain("/shadow={4C1E7A90-2B3D-4E5F-8A6B-1C2D3E4F5A6B}");
  });

  it("sans point de restauration : confirmation explicite exigée, journalisée", async () => {
    render(
      <>
        <CleanupScreen />
        <CleanDialog />
      </>,
    );
    tab("Gouffres système");
    fireEvent.click(await screen.findByRole("checkbox", { name: /WinSxS/ }));
    act(() => setRestorePoint(false));
    fireEvent.click(screen.getByRole("button", { name: "Préparer les actions…" }));
    const go = (await screen.findByRole("button", { name: "Lancer 1 action système" })) as HTMLButtonElement;
    expect(go.disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: "Continuer sans point de restauration" }));
    expect(go.disabled).toBe(false);
    fireEvent.click(go);
    expect(await screen.findByText(/Journal complet : .*dism-.*\.log/, undefined, { timeout: 3000 })).toBeTruthy();
    const journal = await backend.journal();
    expect(journal[0]?.withoutRestorePoint).toBe(true);
  });

  it("protection du système désactivée : expliqué, réglages proposés, puis continuer sans point", async () => {
    localStorage.setItem("lumen-dev-no-protection", "1");
    try {
      render(
        <>
          <CleanupScreen />
          <CleanDialog />
        </>,
      );
      tab("Gouffres système");
      expect(await screen.findByText(/Windows ne peut pas créer de point de restauration sur ce disque \(C:\)/)).toBeTruthy();
      expect(useAdvanced.getState().restorePoint).toBe(false);
      // Point de restauration demandé malgré tout : il échoue, rien d'autre n'est lancé.
      act(() => setRestorePoint(true));
      fireEvent.click(screen.getByRole("checkbox", { name: /WinSxS/ }));
      fireEvent.click(screen.getByRole("button", { name: "Préparer les actions…" }));
      fireEvent.click(await screen.findByRole("button", { name: "Lancer 1 action système" }));
      expect(await screen.findByText(/^Windows ne peut pas créer de point de restauration sur ce disque\. /, undefined, { timeout: 3000 })).toBeTruthy();
      const dialog = screen.getByRole("dialog");
      expect(within(dialog).getByRole("button", { name: "Ouvrir la protection du système" })).toBeTruthy();
      fireEvent.click(within(dialog).getByRole("button", { name: "Continuer sans point de restauration…" }));
      const go = (await screen.findByRole("button", { name: "Lancer 1 action système" })) as HTMLButtonElement;
      expect(go.disabled).toBe(true);
      expect(useStore.getState().clean.plan?.withoutRestorePoint).toBe(true);
    } finally {
      localStorage.removeItem("lumen-dev-no-protection");
    }
  });
});
