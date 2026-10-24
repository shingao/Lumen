// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import type { SearchHit } from "../../ipc/types";
import { Palette } from "./Palette";

const hit: SearchHit = { scanId: 3, id: 42, name: "Mariage Julie.mp4", path: "C:\\Users\\Arthur\\Videos\\Mariage Julie.mp4", size: 9 << 30, modified: Date.now(), kind: "file", drive: "C:", target: "userProfile" };

describe("Palette Ctrl K : recherche", () => {
  beforeEach(() => {
    // jsdom ne fait pas défiler.
    Element.prototype.scrollIntoView = () => {};
    useStore.setState({ locale: "fr", paletteOpen: true });
    vi.spyOn(backend, "search").mockResolvedValue([hit]);
  });

  afterEach(() => vi.restoreAllMocks());

  const type = async (q: string) => {
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: q } });
    await screen.findByText("Mariage Julie.mp4");
    return input;
  };

  it("filtres affichés en pastilles ; une requête filtrée ne propose pas de commandes", async () => {
    render(<Palette />);
    await type("mariage ext:mp4 >1go disque:C");
    const chips = screen.getByLabelText("Filtres de la recherche");
    expect(chips.textContent).toContain(".mp4");
    expect(chips.textContent).toContain("disque C:");
    expect(chips.textContent?.replace(/[\u202f\u00a0]/g, " ")).toContain("> 1,0 Go");
    expect(screen.queryByText("Commandes")).toBeNull();
    expect(backend.search).toHaveBeenLastCalledWith("mariage ext:mp4 >1go disque:C", 12);
  });

  it("Entrée ouvre l'élément, Ctrl+Entrée le montre dans l'Explorateur", async () => {
    const open = vi.spyOn(backend, "openItem").mockResolvedValue("open");
    const { unmount } = render(<Palette />);
    const input = await type("ext:mp4");
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(open).toHaveBeenCalledWith(3, 42, "open"));
    unmount();
    render(<Palette />);
    const input2 = await type("ext:mp4");
    fireEvent.keyDown(input2, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(open).toHaveBeenLastCalledWith(3, 42, "reveal"));
  });

  it("un programme n'est jamais lancé : montré dans l'Explorateur, avec un message", async () => {
    vi.spyOn(backend, "openItem").mockResolvedValue("reveal");
    render(<Palette />);
    const input = await type("ext:mp4");
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(useStore.getState().toast?.text).toMatch(/ne lance jamais un programme/));
  });

  it("Maj+Entrée zoome dans la treemap (écran Analyse)", async () => {
    useStore.setState({ screen: "games" });
    render(<Palette />);
    const input = await type("ext:mp4");
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    await waitFor(() => expect(useStore.getState().screen).toBe("analyse"));
  });
});
