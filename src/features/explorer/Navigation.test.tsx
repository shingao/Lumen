// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { goBack, goForward, goTo, goToTarget, resetNavigation, useNav } from "../../app/navigation";
import { startScan } from "../../app/scanning";
import { navigationButton, navigationKey } from "../../app/shortcuts";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import { Explorer } from "./Explorer";

async function scannedProfile() {
  useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS }, clean: CLEAN_CLOSED, node: null, target: { kind: "userProfile" } });
  await act(() => startScan({ kind: "userProfile" }));
  await waitFor(() => expect(useStore.getState().scan.status).toBe("done"));
}

const idOf = async (path: string) => {
  const scanId = useStore.getState().scan.summary?.scanId ?? 0;
  const id = await backend.findNode(scanId, path);
  if (id === null) throw new Error(path);
  return id;
};

describe("Précédent / suivant dans l'Explorer", () => {
  beforeEach(async () => {
    resetNavigation();
    await scannedProfile();
  });

  it("historique des zooms, boutons grisés et infobulles « Précédent : … »", async () => {
    render(<Explorer />);
    const back = await screen.findByRole("button", { name: "Précédent" });
    const forward = screen.getByRole("button", { name: "Suivant" });
    expect((back as HTMLButtonElement).disabled).toBe(true);
    expect((forward as HTMLButtonElement).disabled).toBe(true);
    await waitFor(() => expect(useNav.getState().here?.label).toBe("Arthur"));

    const videos = await idOf("C:\\Users\\Arthur\\Vidéos");
    act(() => goTo(videos));
    await waitFor(() => expect(useNav.getState().here?.label).toBe("Vidéos"));
    const captures = await idOf("C:\\Users\\Arthur\\Vidéos\\Captures");
    act(() => goTo(captures));
    await waitFor(() => expect(useNav.getState().here?.label).toBe("Captures"));
    const toVideos = screen.getByRole("button", { name: "Précédent : Vidéos" });
    expect(toVideos.getAttribute("title")).toBe("Précédent : Vidéos");

    fireEvent.click(toVideos);
    expect(useStore.getState().node).toBe(videos);
    await screen.findByRole("button", { name: "Suivant : Captures" });
    act(() => goBack());
    expect(useStore.getState().node).toBeNull();
    await screen.findByRole("button", { name: "Suivant : Vidéos" });
    expect((screen.getByRole("button", { name: "Précédent" }) as HTMLButtonElement).disabled).toBe(true);
    act(() => goForward());
    act(() => goForward());
    await waitFor(() => expect(useStore.getState().node).toBe(captures));
    // Un nouveau zoom efface la suite de l'historique.
    act(() => goBack());
    act(() => goTo(captures));
    await waitFor(() => expect(useNav.getState().forward).toHaveLength(0));
  });

  it("changement de disque dans l'historique : retour réanalyse et rouvre le dossier par son chemin", async () => {
    render(<Explorer />);
    const videos = await idOf("C:\\Users\\Arthur\\Vidéos");
    act(() => goTo(videos));
    await waitFor(() => expect(useNav.getState().here?.label).toBe("Vidéos"));
    act(() => goToTarget({ kind: "volume", letter: "D:" }));
    await waitFor(() => expect(useStore.getState().scan.summary?.volume).toBe("D:"));
    await waitFor(() => expect(useNav.getState().here?.target).toEqual({ kind: "volume", letter: "D:" }));
    expect(useNav.getState().back.at(-1)?.label).toBe("Vidéos");

    act(() => goBack());
    await waitFor(() => expect(useStore.getState().target).toEqual({ kind: "userProfile" }));
    await waitFor(() => expect(useStore.getState().scan.status).toBe("done"));
    // Nouvelle analyse : les identifiants ont pu changer, le dossier est retrouvé par son chemin.
    await waitFor(() => expect(useStore.getState().node).toBe(videos));
  });

  it("raccourcis : Alt+← / Alt+→, Retour arrière hors saisie, boutons 4 et 5 de la souris", () => {
    const key = (init: KeyboardEventInit, target?: EventTarget) => {
      const e = new KeyboardEvent("keydown", init);
      if (target) Object.defineProperty(e, "target", { value: target });
      return navigationKey(e);
    };
    expect(key({ key: "ArrowLeft", altKey: true })).toBe("back");
    expect(key({ key: "ArrowRight", altKey: true })).toBe("forward");
    expect(key({ key: "ArrowLeft" })).toBeNull();
    expect(key({ key: "Backspace" }, document.body)).toBe("back");
    expect(key({ key: "Backspace" }, document.createElement("input"))).toBeNull();
    expect(navigationButton(new MouseEvent("mouseup", { button: 3 }))).toBe("back");
    expect(navigationButton(new MouseEvent("mouseup", { button: 4 }))).toBe("forward");
    expect(navigationButton(new MouseEvent("mouseup", { button: 0 }))).toBeNull();
  });
});
