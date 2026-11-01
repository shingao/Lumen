// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useAdvanced } from "../../app/advanced";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { ScanSummary } from "../../ipc/types";
import { CleanupScreen } from "./CleanupScreen";

async function scanned(): Promise<number> {
  const scanId = await backend.startScan({ kind: "userProfile" }, "auto");
  const summary = await new Promise<ScanSummary>((resolve) => {
    const off = backend.onScanEvent((e) => {
      if (e.type === "done" && e.summary.scanId === scanId) {
        off();
        resolve(e.summary);
      }
    });
  });
  useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS }, scan: { id: scanId, status: "done", method: "auto", progress: null, summary, message: null }, clean: CLEAN_CLOSED });
  return scanId;
}

const group = (path: string) => screen.getByRole("region", { name: path });
const toggle = (path: string) => within(group(path)).getByRole("button", { name: new RegExp(`^(Déplier|Replier) ${path.replace(/\\/g, "\\\\")}$`) });
const files = (path: string) => within(group(path)).queryAllByRole("listitem");

describe("Nettoyage : groupes repliés par défaut", () => {
  beforeEach(async () => {
    useStore.setState({ cleanupTab: "dormant" });
    await scanned();
  });

  it("repli par défaut, ligne principale complète, dépliage par la flèche ou la ligne, Tout déplier", async () => {
    render(<CleanupScreen />);
    await screen.findByRole("region", { name: "~\\Videos\\Mariage Julie" });
    const mariage = group("~\\Videos\\Mariage Julie");
    // Repliés à l'arrivée : aucun fichier affiché, la ligne principale résume le groupe.
    expect(files("~\\Videos\\Mariage Julie")).toHaveLength(0);
    expect(toggle("~\\Videos\\Mariage Julie").getAttribute("aria-expanded")).toBe("false");
    const head = mariage.firstElementChild as HTMLElement;
    expect(head.textContent).toContain("3 fichiers");
    expect(head.textContent).toContain("35,1 Go");
    expect(head.textContent).toMatch(/récent : il y a \d+ (mois|ans?)/);
    // La flèche déplie ; un clic sur la ligne replie.
    fireEvent.click(toggle("~\\Videos\\Mariage Julie"));
    expect(files("~\\Videos\\Mariage Julie")).toHaveLength(3);
    fireEvent.click(head);
    expect(files("~\\Videos\\Mariage Julie")).toHaveLength(0);
    // Tout déplier / Tout replier.
    fireEvent.click(screen.getByRole("button", { name: "Tout déplier" }));
    expect(files("~\\Downloads")).toHaveLength(3);
    fireEvent.click(screen.getByRole("button", { name: "Tout replier" }));
    expect(files("~\\Downloads")).toHaveLength(0);
  });

  it("case du groupe partielle, sélection conservée au repli, clavier (← → Espace)", async () => {
    render(<CleanupScreen />);
    await screen.findByRole("region", { name: "~\\Downloads" });
    fireEvent.click(toggle("~\\Downloads"));
    const [box, first] = within(group("~\\Downloads")).getAllByRole("checkbox") as HTMLInputElement[];
    fireEvent.click(first as HTMLInputElement);
    expect((box as HTMLInputElement).indeterminate).toBe(true);
    expect((box as HTMLInputElement).checked).toBe(false);
    // Replié : la sélection reste (case du groupe toujours « en partie »).
    fireEvent.click(toggle("~\\Downloads"));
    expect(files("~\\Downloads")).toHaveLength(0);
    expect(useAdvanced.getState().checkedDormant.size).toBe(1);
    expect((within(group("~\\Downloads")).getByRole("checkbox") as HTMLInputElement).indeterminate).toBe(true);
    // Clavier sur la flèche : Espace coche tout le groupe, → déplie, ← replie.
    const arrow = toggle("~\\Downloads");
    fireEvent.keyDown(arrow, { key: " " });
    expect(useAdvanced.getState().checkedDormant.size).toBe(3);
    expect((within(group("~\\Downloads")).getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
    fireEvent.keyDown(arrow, { key: "ArrowRight" });
    expect(files("~\\Downloads")).toHaveLength(3);
    fireEvent.keyDown(arrow, { key: "ArrowLeft" });
    expect(files("~\\Downloads")).toHaveLength(0);
    fireEvent.keyDown(arrow, { key: " " });
    expect(useAdvanced.getState().checkedDormant.size).toBe(0);
  });

  it("état mémorisé pendant la session, replié à chaque nouvelle analyse ; Gros fichiers par dossier", async () => {
    const { unmount } = render(<CleanupScreen />);
    await screen.findByRole("region", { name: "~\\Downloads" });
    fireEvent.click(toggle("~\\Downloads"));
    unmount();
    // Retour sur l'onglet : toujours déplié.
    const again = render(<CleanupScreen />);
    expect(files("~\\Downloads")).toHaveLength(3);
    again.unmount();
    // Nouvelle analyse : tout est replié.
    await scanned();
    render(<CleanupScreen />);
    await screen.findByRole("region", { name: "~\\Downloads" });
    expect(files("~\\Downloads")).toHaveLength(0);
    // Gros fichiers : un groupe par dossier, replié.
    fireEvent.click(screen.getByRole("button", { name: "Gros fichiers" }));
    const videos = await screen.findByRole("region", { name: "~\\Videos\\Mariage Julie" });
    expect(within(videos).queryAllByRole("listitem")).toHaveLength(0);
    expect((videos.firstElementChild as HTMLElement).textContent).toContain("3 fichiers");
  });
});
