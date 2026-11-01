// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { Treemap } from "./Treemap";

describe("Treemap", () => {
  it("dessine les tuiles du dossier, hachure les sans risque et estompe hors sélection", async () => {
    useStore.setState({ locale: "fr", colorMode: "category" });
    backend.setLocale("fr");
    const scanId = await backend.startScan({ kind: "userProfile" }, "auto");
    const { root } = await backend.tree(scanId, null);
    const recos = await backend.recommendations(scanId);
    const npm = recos.find((r) => r.id === "npm");
    render(<Treemap root={root} highlight={npm?.highlight ?? null} onCopyPath={() => {}} onUnavailable={() => {}} />);

    // Cache npm : règle opaque, une seule tuile (« Contenu interne du cache npm »), hachurée.
    const npmTile = screen.getByRole("button", { name: /^Contenu interne du cache npm,/ });
    expect(npmTile.dataset.hatch).toBeDefined();
    expect(npmTile.dataset.opaque).toBeDefined();
    expect(npmTile.dataset.dim).toBeUndefined();
    expect(npmTile.querySelector("[data-node]")).toBeNull();
    // Libellé long : le gain passe sur sa propre ligne sous le titre, jamais tronqué.
    const head = npmTile.querySelector("[data-stacked]");
    expect(head?.textContent).toMatch(/^Contenu interne du cache npm−/);
    const projects = screen.getByRole("button", { name: /^Projets,/ });
    expect(projects.dataset.hatch).toBeUndefined();
    expect(projects.dataset.dim).toBeDefined();
    expect(projects.style.getPropertyValue("--tile-bg")).toBe("var(--cat-code)");
  });

  it("zone entièrement couverte : hachures au plus haut seulement, descendants unis avec un bord", async () => {
    useStore.setState({ locale: "fr", colorMode: "category" });
    backend.setLocale("fr");
    const scanId = await backend.startScan({ kind: "userProfile" }, "auto");
    const gradle = await backend.findNode(scanId, "C:\\Users\\Arthur\\.gradle");
    const { root } = await backend.tree(scanId, gradle);
    render(<Treemap root={root} highlight={null} onCopyPath={() => {}} onUnavailable={() => {}} />);
    const caches = screen.getByRole("button", { name: /^caches,/ });
    // Niveau 1 sous .gradle (le plus haut couvert) : plus de hachures, fond uni de statut, bord.
    expect(caches.dataset.hatch).toBeUndefined();
    expect(caches.dataset.cover).toBe("1");
    expect(caches.style.getPropertyValue("--tile-bg")).toBe("var(--st-safe)");
    expect(caches.style.getPropertyValue("--cover-border")).toBe("var(--cover-border-safe)");
    // Niveau 2 : un pas plus clair.
    const modules = caches.querySelector<HTMLElement>("[data-node][data-cover='2']");
    expect(modules?.textContent).toContain("modules-2");
    expect(modules?.dataset.hatch).toBeUndefined();
  });
});
