// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { hideTip, initOnboarding, resetTips, useOnboarding, useTip } from "../../app/onboarding";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import { SettingsScreen } from "../settings/SettingsScreen";
import { TipAnchor } from "../tips/TipBubble";
import { Onboarding } from "./Onboarding";

const STEPS = [
  "Voici où passe ton espace disque.",
  "Les zones hachurées peuvent partir sans risque.",
  "Coche ce que tu veux libérer, Lumen te montre exactement ce qu'il va faire.",
  "Rien n'est supprimé sans ton accord, et tout passe par la Corbeille.",
];

async function fresh() {
  // Le backend factice garde ses propres réglages : langue fixée (sinon celle du système, l'anglais).
  await backend.updateSettings({ language: "fr", onboardingDone: false, tipsSeen: [] });
  useStore.setState({
    locale: "fr",
    screen: "analyse",
    settingsOpen: false,
    settings: { ...DEFAULT_SETTINGS, language: "fr", onboardingDone: false, tipsSeen: [] },
    scan: { id: null, status: "idle", method: "auto", progress: null, summary: null, message: null },
  });
  useOnboarding.setState({ open: false, step: 0 });
  hideTip();
}

/** Réglages identiques côté backend factice et côté store. */
async function settingsTo(patch: Partial<typeof DEFAULT_SETTINGS>) {
  const settings = await backend.updateSettings({ language: "fr", ...patch });
  useStore.setState({ settings });
}

const done = () => useStore.getState().settings?.onboardingDone;

describe("Tutoriel de bienvenue", () => {
  beforeEach(fresh);

  it("s'ouvre au premier lancement seulement", () => {
    initOnboarding({ ...DEFAULT_SETTINGS, language: "fr", onboardingDone: true });
    expect(useOnboarding.getState().open).toBe(false);
    initOnboarding({ ...DEFAULT_SETTINGS, language: "fr", onboardingDone: false });
    expect(useOnboarding.getState().open).toBe(true);
  });

  it("séquence complète : 4 écrans, Précédent, puis « Voir mon analyse » le ferme pour de bon", async () => {
    initOnboarding(useStore.getState().settings);
    render(<Onboarding />);
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByRole("button", { name: "Passer" })).toBeTruthy();
    for (let i = 0; i < 3; i++) {
      expect(screen.getByText(STEPS[i] ?? "")).toBeTruthy();
      expect(screen.getByText(`${i + 1} / 4`)).toBeTruthy();
      // « Passer » reste visible à chaque écran.
      expect(screen.getByRole("button", { name: "Passer" })).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Suivant" }));
    }
    expect(screen.getByText(STEPS[3] ?? "")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Précédent" }));
    expect(screen.getByText(STEPS[2] ?? "")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Suivant" }));
    // Le bouton principal a le focus à chaque écran.
    expect(document.activeElement?.textContent).toBe("Voir mon analyse");
    fireEvent.click(screen.getByRole("button", { name: "Voir mon analyse" }));
    expect(useOnboarding.getState().open).toBe(false);
    await waitFor(() => expect(done()).toBe(true));
  });

  it("« Passer » ferme tout de suite, depuis n'importe quel écran", async () => {
    useOnboarding.setState({ open: true, step: 1 });
    render(<Onboarding />);
    fireEvent.click(screen.getByRole("button", { name: "Passer" }));
    expect(useOnboarding.getState().open).toBe(false);
    await waitFor(() => expect(done()).toBe(true));
  });

  it("clavier : flèches pour naviguer, Échap ferme", async () => {
    useOnboarding.setState({ open: true, step: 0 });
    render(<Onboarding />);
    fireEvent.keyDown(window, { key: "ArrowRight" });
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(useOnboarding.getState().step).toBe(2);
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    expect(useOnboarding.getState().step).toBe(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(useOnboarding.getState().open).toBe(false);
    await waitFor(() => expect(done()).toBe(true));
  });

  it("la première analyse tourne en arrière-plan pendant le tutoriel", () => {
    useOnboarding.setState({ open: true, step: 3 });
    useStore.getState().setScan({
      status: "running",
      progress: { scanId: 1, files: 128_400, dirs: 9_000, allocated: 0, errors: 0, elapsedMs: 900, phase: "walk", tops: [] },
    });
    const { rerender } = render(<Onboarding />);
    expect(screen.getByRole("status").textContent?.replace(/[\u202f\u00a0]/g, " ")).toBe(
      "Ton analyse tourne déjà en arrière-plan : 128 400 fichiers…",
    );
    act(() => useStore.getState().setScan({ status: "done" }));
    rerender(<Onboarding />);
    expect(screen.getByText("Ton analyse est prête.")).toBeTruthy();
  });

  it("relançable depuis les Paramètres, même après l'avoir terminé", () => {
    useStore.setState({ settingsOpen: true, settings: { ...DEFAULT_SETTINGS, language: "fr", onboardingDone: true } });
    render(<SettingsScreen />);
    fireEvent.click(screen.getByRole("button", { name: "Relancer le tutoriel" }));
    expect(useStore.getState().settingsOpen).toBe(false);
    expect(useOnboarding.getState()).toEqual({ open: true, step: 0 });
  });
});

function Nav() {
  return (
    <TipAnchor>
      <nav>
        <button type="button" aria-current="page">
          Analyse
        </button>
      </nav>
    </TipAnchor>
  );
}

describe("Astuces contextuelles", () => {
  beforeEach(async () => {
    await fresh();
    await settingsTo({ onboardingDone: true, tipsSeen: [] });
  });
  afterEach(() => hideTip());

  const tip = "Clique sur une tuile pour zoomer, survole une recommandation pour la voir dans la treemap. Ctrl K retrouve n'importe quel fichier.";

  it("une seule fois par écran, fermée en un clic", async () => {
    const first = render(<Nav />);
    const note = await screen.findByRole("note", { name: "Astuce" });
    expect(note.textContent).toContain(tip);
    await waitFor(() => expect(useStore.getState().settings?.tipsSeen).toEqual(["analyse"]));
    fireEvent.click(note);
    expect(screen.queryByRole("note")).toBeNull();
    first.unmount();
    // Prochaine visite de l'écran : plus d'astuce.
    render(<Nav />);
    expect(screen.queryByRole("note")).toBeNull();
  });

  it("chaque écran a la sienne ; quitter l'écran la retire sans la répéter", async () => {
    render(<Nav />);
    await screen.findByRole("note");
    act(() => useStore.getState().go("journal"));
    expect(await screen.findByText(/se restaure d'un clic/)).toBeTruthy();
    expect(screen.queryByText(tip)).toBeNull();
    act(() => useStore.getState().go("analyse"));
    await waitFor(() => expect(screen.queryByRole("note")).toBeNull());
    expect(useTip.getState().screen).toBeNull();
  });

  it("jamais pendant le tutoriel ; « Réinitialiser les astuces » les fait revenir", async () => {
    await settingsTo({ onboardingDone: false, tipsSeen: [] });
    useOnboarding.setState({ open: true, step: 0 });
    const r = render(<Nav />);
    expect(screen.queryByRole("note")).toBeNull();
    r.unmount();
    useOnboarding.setState({ open: false });
    await settingsTo({ onboardingDone: true, tipsSeen: ["analyse", "journal"] });
    const again = render(<Nav />);
    expect(screen.queryByRole("note")).toBeNull();
    again.unmount();
    await act(() => resetTips());
    expect(useStore.getState().settings?.tipsSeen).toEqual([]);
    expect(useStore.getState().toast?.text).toBe("Les astuces réapparaîtront à la prochaine visite de chaque écran");
    render(<Nav />);
    expect(await screen.findByRole("note")).toBeTruthy();
  });

  it("bouton « Réinitialiser les astuces » dans les Paramètres", async () => {
    await settingsTo({ onboardingDone: true, tipsSeen: ["analyse"] });
    render(<SettingsScreen />);
    fireEvent.click(screen.getByRole("button", { name: "Réinitialiser les astuces" }));
    await waitFor(() => expect(useStore.getState().settings?.tipsSeen).toEqual([]));
  });
});
