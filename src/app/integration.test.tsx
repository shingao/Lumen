// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { backend } from "../ipc/client";
import { DEFAULT_SETTINGS, mockIntegration } from "../ipc/mock";
import type { Settings } from "../ipc/types";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { checkLowSpace, handleOpen, initIntegration, resetIntegration, setAutostart, useIntegration } from "./integration";
import { resetNavigation } from "./navigation";
import { useStore } from "./store";

const mock = () => {
  const m = mockIntegration();
  if (!m) throw new Error("backend factice absent");
  return m;
};

async function fresh(patch: Partial<Settings> = {}) {
  resetIntegration();
  resetNavigation();
  const settings = await backend.updateSettings({ ...DEFAULT_SETTINGS, language: "fr", onboardingDone: true, alertsSent: {}, ...patch });
  useStore.setState({
    locale: "fr",
    screen: "journal",
    settingsOpen: false,
    settings,
    target: { kind: "userProfile" },
    node: null,
    toast: null,
    scan: { id: null, status: "idle", method: "auto", progress: null, summary: null, message: null },
  });
  mock().log.notifications.length = 0;
  mock().log.errors.length = 0;
  mock().log.applied = null;
}

const DAY = new Date(2026, 9, 28, 12).getTime();

describe("« Analyser avec Lumen » (menu contextuel)", () => {
  beforeEach(() => fresh());
  afterEach(() => resetIntegration());

  it("ouvre l'Explorer de Lumen sur le dossier, dans l'analyse de son disque", async () => {
    await initIntegration();
    act(() => mock().simulate.open("C:\\Users\\Arthur\\Vidéos"));
    await waitFor(() => expect(useStore.getState().scan.status).toBe("done"), { timeout: 3000 });
    const s = useStore.getState();
    expect(s.screen).toBe("analyse");
    expect(s.target).toEqual({ kind: "volume", letter: "C:" });
    const id = s.scan.summary ? await backend.findNode(s.scan.summary.scanId, "C:\\Users\\Arthur\\Vidéos") : null;
    await waitFor(() => expect(useStore.getState().node).toBe(id));
    expect(id).not.toBeNull();
  });

  it("refuse un partage réseau ou un lecteur inconnu, avec un message", async () => {
    await handleOpen({ kind: "refused", reason: "network" });
    expect(useStore.getState().toast?.text).toBe("Impossible d'ouvrir ce dossier : les partages réseau ne sont pas analysés.");
    await initIntegration();
    act(() => mock().simulate.open("Q:\\Absent"));
    await waitFor(() => expect(useStore.getState().toast?.text).toBe("Impossible d'ouvrir ce dossier : lecteur absent ou non analysable."));
    expect(useStore.getState().screen).toBe("journal");
  });
});

describe("zone de notification et démarrage", () => {
  beforeEach(() => fresh());
  afterEach(() => resetIntegration());

  it("icône désactivée par défaut, puis infobulle de l'espace libre du disque principal", async () => {
    await initIntegration();
    await waitFor(() => expect(mock().log.applied).not.toBeNull());
    expect(mock().log.applied?.tray).toBe(false);
    await act(async () => {
      await backend.updateSettings({ trayIcon: true }).then((settings) => useStore.setState({ settings }));
    });
    await waitFor(() => expect(mock().log.applied?.tray).toBe(true));
    const applied = mock().log.applied;
    expect(applied?.tooltip.replace(/[\u202f\u00a0]/g, " ")).toBe("Lumen — 73,8 Go libres sur 476 Go (C:)");
    expect(applied?.labels).toEqual({ open: "Ouvrir Lumen", scan: "Analyser C:", quit: "Quitter" });
    expect(applied?.menuLabel).toBe("Analyser avec Lumen");
  });

  it("« Analyser » du menu de l'icône lance l'analyse du disque principal", async () => {
    await initIntegration();
    act(() => mock().simulate.event({ type: "scan", letter: "C:" }));
    expect(useStore.getState().screen).toBe("analyse");
    expect(useStore.getState().target).toEqual({ kind: "volume", letter: "C:" });
  });

  it("menu contextuel et démarrage : lus et écrits dans le registre, depuis les Paramètres", async () => {
    await initIntegration();
    await setAutostart(false);
    await act(() => mock().setExplorerMenu(true).then((info) => useIntegration.setState({ info })));
    useStore.setState({ settingsOpen: true });
    render(<SettingsScreen />);
    const menu = screen.getByRole("switch", { name: "« Analyser avec Lumen » dans l'Explorateur" });
    const autostart = screen.getByRole("switch", { name: "Lancer au démarrage de Windows" });
    expect(menu.getAttribute("aria-checked")).toBe("true");
    expect(autostart.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(menu);
    fireEvent.click(autostart);
    await waitFor(() => expect(useIntegration.getState().info).toMatchObject({ explorerMenu: false, autostart: true }));
    expect(await backend.integration()).toMatchObject({ explorerMenu: false, autostart: true });
  });
});

describe("notifications d'espace libre", () => {
  afterEach(() => resetIntegration());

  it("désactivées par défaut", async () => {
    await fresh();
    expect(DEFAULT_SETTINGS.lowSpaceAlert).toBe(false);
    expect(await checkLowSpace(await backend.volumes(), DAY)).toBe(0);
    expect(mock().log.notifications).toEqual([]);
  });

  it("sous le seuil : une par disque et par jour, texte en français", async () => {
    await fresh({ lowSpaceAlert: true, lowSpaceThreshold: 10 });
    const volumes = await backend.volumes();
    expect(await checkLowSpace(volumes, DAY)).toBe(1);
    const n = mock().log.notifications[0];
    expect(n?.letter).toBe("D:");
    expect(n?.title).toBe("Espace disque faible sur D:");
    expect(n?.body.replace(/[\u202f\u00a0]/g, " ")).toBe("Plus que 44,0 Go libres (5 %). Clique pour voir ce qui prend la place.");
    // Même jour : rien de plus. Le lendemain : de nouveau.
    expect(await checkLowSpace(volumes, DAY + 3_600_000)).toBe(0);
    expect(await checkLowSpace(volumes, DAY + 86_400_000)).toBe(1);
    // Seuil plus haut : le disque système aussi (15,5 % libres).
    await backend.updateSettings({ lowSpaceThreshold: 20, alertsSent: {} }).then((settings) => useStore.setState({ settings }));
    expect(await checkLowSpace(volumes, DAY)).toBe(2);
  });

  it("un clic sur la notification ouvre l'Analyse du disque concerné", async () => {
    await fresh();
    await initIntegration();
    act(() => mock().simulate.event({ type: "alert", letter: "D:" }));
    await waitFor(() => expect(useStore.getState().target).toEqual({ kind: "volume", letter: "D:" }));
    expect(useStore.getState().screen).toBe("analyse");
  });
});

describe("journal de plantage et À propos", () => {
  beforeEach(() => fresh());
  afterEach(() => resetIntegration());

  it("les erreurs non rattrapées de l'interface vont au journal local", async () => {
    await initIntegration();
    window.dispatchEvent(new ErrorEvent("error", { message: "TypeError: x is undefined", error: new TypeError("x is undefined") }));
    expect(mock().log.errors[0]).toContain("TypeError: x is undefined");
  });

  it("version, licence, dépôt et licences des composants tiers", async () => {
    useStore.setState({ settingsOpen: true });
    render(<SettingsScreen />);
    const about = screen.getByRole("region", { name: "À propos" });
    expect(about.textContent).toContain("0.9.0");
    expect(about.textContent).toContain("Pas de mise à jour automatique");
    expect(about.textContent).toContain("https://github.com/shingao/Lumen");
    expect(about.textContent).toMatch(/\d+ crates Rust et \d+ paquets npm/);
    fireEvent.click(screen.getByRole("button", { name: "Afficher" }));
    const dialog = screen.getByRole("dialog", { name: "Licences des composants tiers" });
    await waitFor(() => expect(dialog.textContent).toContain("Lumen — composants tiers"));
    expect(dialog.textContent).toContain("react");
    fireEvent.click(screen.getByRole("button", { name: "Fermer" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
