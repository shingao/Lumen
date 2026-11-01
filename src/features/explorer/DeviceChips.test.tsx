// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetNavigation } from "../../app/navigation";
import { startScan } from "../../app/scanning";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { useVolumeList } from "../../app/volumes";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS, mockDevices } from "../../ipc/mock";
import { Recommendations } from "../recommendations/Recommendations";
import { Explorer } from "./Explorer";

const devices = () => {
  const d = mockDevices();
  if (!d) throw new Error("backend factice absent");
  return d;
};
const chips = () => screen.getByRole("group", { name: "Volumes" });
const chip = (name: RegExp) => within(chips()).getByRole("button", { name });

describe("Périphériques dans la barre de l'Explorer", () => {
  beforeEach(async () => {
    resetNavigation();
    devices().unplug("E:");
    useVolumeList.setState({ list: null });
    useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS }, clean: CLEAN_CLOSED, node: null, target: { kind: "userProfile" }, toast: null });
    await act(() => startScan({ kind: "userProfile" }));
    await waitFor(() => expect(useStore.getState().scan.status).toBe("done"));
  });

  afterEach(() => vi.restoreAllMocks());

  it("une puce par volume ; une clé branchée apparaît à chaud, analyse et export seulement", async () => {
    render(
      <>
        <Explorer />
        <Recommendations />
      </>,
    );
    // Dossier utilisateur, C:, D:, Z: et « Actualiser ».
    await waitFor(() => expect(within(chips()).getAllByRole("button")).toHaveLength(5));
    expect(chip(/^C: Système, interne/)).toBeTruthy();
    expect(chip(/^Z: partage/).textContent).toContain("réseau");
    // Ancien menu déroulant remplacé.
    expect(screen.queryByRole("button", { name: /Disque analysé/ })?.getAttribute("aria-haspopup")).toBeFalsy();

    act(() => devices().plug());
    const usb = await waitFor(() => chip(/^E: KINGSTON, USB/));
    expect(useStore.getState().toast?.text).toBe("E: KINGSTON branché : un clic sur sa puce l'analyse.");

    const start = vi.spyOn(backend, "startScan");
    fireEvent.click(usb);
    await waitFor(() => expect(start).toHaveBeenCalledWith({ kind: "volume", letter: "E:" }, "auto"));
    await waitFor(() => expect(useStore.getState().scan.status).toBe("done"));
    expect(screen.getByRole("note").textContent).toBe("Pas de Corbeille sur ce support : Lumen ne propose aucun nettoyage ici. Analyse et export seulement.");
    // Aucun nettoyage : ni case, ni bouton Nettoyer ni script.
    expect(screen.queryByRole("button", { name: "Nettoyer" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Copier le script" })).toBeNull();
    expect(screen.getByText("Pas de Corbeille sur ce support : aucun nettoyage proposé.")).toBeTruthy();
    // Export toujours possible.
    expect(screen.getByRole("button", { name: "Exporter" })).toBeTruthy();
    // La puce dit quand le volume a été analysé.
    await waitFor(() => expect(chip(/^E: KINGSTON/).textContent).toMatch(/analysé il y a/));
  });

  it("débranchement pendant l'analyse : arrêt propre, message clair, la puce disparaît", async () => {
    render(<Explorer />);
    act(() => devices().plug());
    await waitFor(() => chip(/^E: KINGSTON/));
    // Nouvelle analyse de la clé (« Analyser à nouveau » : jamais celle gardée en mémoire).
    act(() => {
      void startScan({ kind: "volume", letter: "E:" });
    });
    await waitFor(() => expect(useStore.getState().scan.status).toBe("running"));
    act(() => devices().unplug("E:"));
    await waitFor(() => expect(useStore.getState().scan.status).not.toBe("running"));
    expect(useStore.getState().scan.message).toBe("le support E: a été débranché pendant l'analyse : rien n'a été modifié");
    await waitFor(() => expect(within(chips()).queryByRole("button", { name: /^E:/ })).toBeNull());
    expect(useStore.getState().toast?.text).toBe("E: KINGSTON débranché.");
    // L'analyse arrêtée ne revient pas plus tard.
    await new Promise((r) => setTimeout(r, 500));
    expect(useStore.getState().scan.summary?.volume).not.toBe("E:");
  });

  it("lecteur réseau : analysé seulement sur clic confirmé, avec l'avertissement de lenteur", async () => {
    render(<Explorer />);
    const start = vi.spyOn(backend, "startScan");
    fireEvent.click(await waitFor(() => chip(/^Z:/)));
    const dialog = await screen.findByRole("dialog", { name: "Analyser Z: ?" });
    expect(dialog.textContent).toContain("peut être lente");
    expect(start).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Analyser quand même" }));
    await waitFor(() => expect(start).toHaveBeenCalledWith({ kind: "volume", letter: "Z:" }, "auto"));
  });

  it("analyse récente en cache : affichée tout de suite, sans réanalyser", async () => {
    render(<Explorer />);
    fireEvent.click(await waitFor(() => chip(/^D: Données/)));
    await waitFor(() => expect(useStore.getState().scan.summary?.volume).toBe("D:"));
    fireEvent.click(chip(/^C: Système/));
    await waitFor(() => {
      const { target, scan } = useStore.getState();
      expect(target).toEqual({ kind: "volume", letter: "C:" });
      expect(scan.status === "done" && scan.summary !== null).toBe(true);
    });
    const start = vi.spyOn(backend, "startScan");
    fireEvent.click(chip(/^D: Données/));
    await waitFor(() => expect(useStore.getState().scan.summary?.volume).toBe("D:"));
    expect(start).not.toHaveBeenCalled();
    expect(chip(/^D: Données/).textContent).toMatch(/analysé il y a/);
  });

  it("lecture des volumes lente ou bloquée : l'interface reste utilisable, puces « lecture… » et réseau non lu", async () => {
    // Le backend ne répond jamais à la liste (lecteur réseau ou disque qui ne répond pas).
    const never = new Promise<never>(() => undefined);
    vi.spyOn(backend, "volumes").mockReturnValue(never);
    vi.spyOn(backend, "refreshVolumes").mockReturnValue(never);
    render(
      <>
        <Explorer />
        <Recommendations />
      </>,
    );
    // L'Explorer s'affiche et répond : treemap, recommandations, zoom, puce du dossier utilisateur.
    expect(await screen.findByRole("group", { name: "Carte de l'espace disque" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Nettoyer" })).toBeTruthy();
    const user = chip(/^Dossier utilisateur/);
    expect(user.getAttribute("aria-pressed")).toBe("true");
    const before = useStore.getState().scan.summary?.scanId;
    fireEvent.click(user);
    expect(useStore.getState().scan.summary?.scanId).toBe(before);
    // Un branchement signalé par le sondage s'affiche quand même (événement, sans relire la liste).
    act(() => devices().plug());
    await waitFor(() => expect(within(chips()).getByRole("button", { name: /^E: KINGSTON/ })).toBeTruthy());

    // Détails pas encore lus : « lecture… » ; lecteur réseau jamais lu en tâche de fond.
    const pending = (letter: string, kind: "usb" | "removable" | "network") => ({
      id: letter,
      key: `letter:${letter}`,
      letter,
      label: "",
      kind,
      fileSystem: "",
      mftCapable: false,
      total: 0,
      free: 0,
      cleanable: false,
      analyzedAt: null,
      ready: false,
    });
    act(() => useVolumeList.setState({ list: [pending("F:", "removable"), pending("Y:", "network")] }));
    expect(chip(/^F:, amovible, pas encore lu/).textContent).toContain("lecture…");
    expect(chip(/^Y:, réseau, pas encore lu/).textContent).toContain("espace non lu");
  });

  it("« Actualiser » au bout des puces : relit les lecteurs amovibles, met la liste à jour", async () => {
    render(<Explorer />);
    await waitFor(() => expect(within(chips()).getAllByRole("button").length).toBeGreaterThan(1));
    const card = {
      id: "S:",
      key: "guid:{carte}",
      letter: "S:",
      label: "CANON_DC",
      kind: "sd" as const,
      fileSystem: "exFAT",
      mftCapable: false,
      total: 32 * 2 ** 30,
      free: 12 * 2 ** 30,
      cleanable: false,
      analyzedAt: null,
      ready: true,
    };
    const refresh = vi.spyOn(backend, "refreshRemovable").mockResolvedValue([...(useVolumeList.getState().list ?? []), card]);
    const volumes = vi.spyOn(backend, "refreshVolumes");
    const button = within(chips()).getByRole("button", { name: "Actualiser les lecteurs amovibles" });
    expect(button.getAttribute("title")).toContain("jamais les lecteurs réseau");
    fireEvent.click(button);
    expect(refresh).toHaveBeenCalledOnce();
    expect(volumes).not.toHaveBeenCalled();
    expect(await waitFor(() => chip(/^S: CANON_DC, carte SD/))).toBeTruthy();
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
  });
});
