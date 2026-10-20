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

    const npmTile = screen.getByRole("button", { name: /^npm-cache,/ });
    expect(npmTile.dataset.hatch).toBeDefined();
    expect(npmTile.dataset.dim).toBeUndefined();
    const projects = screen.getByRole("button", { name: /^Projets,/ });
    expect(projects.dataset.hatch).toBeUndefined();
    expect(projects.dataset.dim).toBeDefined();
    expect(projects.style.getPropertyValue("--tile-bg")).toBe("var(--cat-code)");
  });
});
