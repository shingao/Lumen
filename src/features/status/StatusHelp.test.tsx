// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { Treemap } from "../explorer/Treemap";
import { KEEP_FOCUS, StatusHelp } from "./StatusHelp";

function Harness() {
  return (
    <div>
      <StatusHelp status="safe" from="kpi" />
      <StatusHelp status="review" from="reco" />
      <div {...{ [KEEP_FOCUS]: "" }}>
        <button type="button">Zoom</button>
      </div>
      <button type="button">Ailleurs</button>
    </div>
  );
}

describe("« ? » d'un statut", () => {
  beforeEach(() => useStore.setState({ locale: "fr", statusFocus: null, node: null }));

  it("un clic met en évidence et explique ; second clic ou Échap arrête ; un clic ailleurs ne ferme que la bulle", () => {
    render(<Harness />);
    const safe = screen.getByRole("button", { name: "Montrer dans la treemap ce qui est sans risque" });
    fireEvent.click(safe);
    expect(useStore.getState().statusFocus).toEqual({ status: "safe", from: "kpi" });
    expect(safe.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("note", { name: "Sans risque" }).textContent).toMatch(/données régénérables/);
    // Second clic : fin.
    fireEvent.click(safe);
    expect(useStore.getState().statusFocus).toBeNull();
    expect(screen.queryByRole("note")).toBeNull();
    // Échap : fin.
    fireEvent.click(safe);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(useStore.getState().statusFocus).toBeNull();
    // Clic ailleurs : la bulle se ferme, la mise en évidence reste (puce ✕, Échap ou « ? »).
    fireEvent.click(safe);
    fireEvent.pointerDown(screen.getByRole("button", { name: "Zoom" }));
    expect(screen.getByRole("note")).toBeTruthy();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Ailleurs" }));
    expect(screen.queryByRole("note")).toBeNull();
    expect(useStore.getState().statusFocus?.status).toBe("safe");
    fireEvent.click(safe);
    expect(useStore.getState().statusFocus).toBeNull();
  });

  it("reste active pendant le zoom et la navigation ; un autre statut la remplace", () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Montrer dans la treemap ce qui est sans risque" }));
    useStore.getState().zoom(42);
    useStore.getState().setTarget({ kind: "volume", letter: "D:" });
    expect(useStore.getState().statusFocus?.status).toBe("safe");
    fireEvent.click(screen.getByRole("button", { name: "Montrer dans la treemap ce qui est à vérifier" }));
    expect(useStore.getState().statusFocus).toEqual({ status: "review", from: "reco" });
    expect(screen.getByRole("note", { name: "À vérifier" }).textContent).toMatch(/jamais coché par défaut/);
  });

  it("treemap : toutes les tuiles du statut restent nettes, le reste est estompé", async () => {
    useStore.setState({ colorMode: "category" });
    backend.setLocale("fr");
    const scanId = await backend.startScan({ kind: "userProfile" }, "auto");
    const { root } = await backend.tree(scanId, null);
    const recos = await backend.recommendations(scanId);
    const ids = recos.filter((r) => r.status === "safe").flatMap((r) => r.highlight);
    render(<Treemap root={root} highlight={ids} highlightStatus="safe" onCopyPath={() => {}} onUnavailable={() => {}} />);
    expect(screen.getByRole("button", { name: /^Contenu interne du cache npm,/ }).dataset.dim).toBeUndefined();
    expect(screen.getByRole("button", { name: /^\.gradle,/ }).dataset.dim).toBeUndefined();
    expect(screen.getByRole("button", { name: /^Vidéos,/ }).dataset.dim).toBeDefined();
    // « À vérifier » (Android) estompé quand on montre le sans risque.
    expect(screen.getByRole("button", { name: /^Android\\Sdk,/ }).dataset.dim).toBeDefined();
    // Sous .gradle, le contenu (même statut) reste net : rien d'estompé deux fois.
    const gradle = screen.getByRole("button", { name: /^\.gradle,/ });
    expect(gradle.querySelector("[data-dim]")).toBeNull();
  });
});
