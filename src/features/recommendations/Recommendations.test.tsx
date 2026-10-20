// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_CHECKED } from "../../ipc/mock/data";
import type { ScanSummary } from "../../ipc/types";
import { Recommendations } from "./Recommendations";

const text = (el: HTMLElement) => (el.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");

describe("Recommandations", () => {
  beforeEach(async () => {
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
      target: { kind: "userProfile" },
      scan: { id: scanId, status: "done", method: "auto", progress: null, summary, message: null },
      checked: [...DEFAULT_CHECKED],
      openReco: null,
      hoverReco: null,
    });
  });

  it("affiche les deux groupes et le total sélectionné de la maquette", async () => {
    render(<Recommendations />);
    expect(await screen.findByText("Cache npm")).toBeTruthy();
    expect(screen.getByText("Sans risque")).toBeTruthy();
    expect(screen.getByText("À vérifier")).toBeTruthy();
    expect(text(screen.getByText(/sélectionnés/))).toBe("43,4 Go sélectionnés · 5 éléments");
  });

  it("aucun élément « À vérifier » n'est coché par défaut", async () => {
    render(<Recommendations />);
    const box = (await screen.findByText("Modèles Ollama")).closest("label")?.querySelector("input");
    expect(box?.checked).toBe(false);
  });

  it("« Tout sélectionner (sans risque) » n'ajoute que les sans risque", async () => {
    render(<Recommendations />);
    await screen.findByText("Cache npm");
    fireEvent.click(screen.getByRole("button", { name: "Tout sélectionner (sans risque)" }));
    const { checked } = useStore.getState();
    expect(checked).toContain("d3d");
    expect(checked).not.toContain("android");
    expect(checked).not.toContain("ollama");
  });

  it("le survol d'une ligne met ses tuiles en avant, le chevron ouvre le détail", async () => {
    render(<Recommendations />);
    const row = (await screen.findByText("Caches Gradle")).closest("[data-hover], div");
    act(() => {
      fireEvent.pointerEnter(row as Element);
    });
    expect(useStore.getState().hoverReco).toBe("gradle");
    fireEvent.click(screen.getByRole("button", { name: "Voir le détail : Caches Gradle" }));
    expect(useStore.getState().openReco).toBe("gradle");
  });
});
