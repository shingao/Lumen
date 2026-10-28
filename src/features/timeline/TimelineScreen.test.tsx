// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { resetNavigation, useNav } from "../../app/navigation";
import { CLEAN_CLOSED, useStore } from "../../app/store";
import { resetTimeline, useTimeline } from "../../app/timeline";
import { GIB } from "../../core/format";
import { makeTranslator } from "../../i18n";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import type { TimelineData } from "../../ipc/types";
import { SettingsScreen } from "../settings/SettingsScreen";
import { projectionText } from "./labels";
import { TimelineScreen } from "./TimelineScreen";

const text = (el: Element | null) => (el?.textContent ?? "").replace(/[\u202f\u00a0]/g, " ").replace(/\u2212/g, "-");
const t = makeTranslator("fr");

function data(projection: TimelineData["projection"]): TimelineData {
  return { points: [], markers: [], projection, free: 70 * GIB, total: 476 * GIB, trend7d: null, base: null, target: null };
}

describe("Timeline", () => {
  beforeEach(() => {
    resetTimeline();
    resetNavigation();
    useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS }, clean: CLEAN_CLOSED, screen: "timeline", target: { kind: "userProfile" } });
  });

  it("courbe de la série du profil : repères des nettoyages, projection, snapshot quotidien", async () => {
    render(<TimelineScreen />);
    await screen.findByText(/1 autre série porte sur ce dossier/);
    const select = screen.getByRole("combobox", { name: "Série" }) as HTMLSelectElement;
    expect(text(select.selectedOptions[0] ?? null)).toMatch(/^C: Système · dossier utilisateur · parcours/);
    // Repère du nettoyage de Lumen d'il y a 12 jours, sur la période par défaut (30 jours).
    expect(screen.getAllByText((_, el) => el?.tagName === "SPAN" && /^-44,2 Go nettoyés$/.test(text(el))).length).toBe(1);
    expect(screen.queryByText(/6,1 Go nettoyés/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "3 mois" }));
    expect(screen.getAllByText((_, el) => el?.tagName === "SPAN" && /^-6,1 Go nettoyés$/.test(text(el))).length).toBe(1);
    expect(text(screen.getByText(/Au rythme des 30 derniers jours/))).toMatch(/par semaine, nettoyages de Lumen exclus\), le disque sera plein dans environ \d+ semaines\./);
    await screen.findByText(/1 série analysée par la MFT n'a pas de snapshot quotidien/);
    expect(screen.getByRole("img", { name: /^Espace utilisé, C: Système/ })).toBeTruthy();
  });

  it("disque dont la lettre a changé, disque absent, projection impossible", async () => {
    render(<TimelineScreen />);
    const select = (await screen.findByRole("combobox", { name: "Série" })) as HTMLSelectElement;
    const option = (label: RegExp) => [...select.options].find((o) => label.test(text(o)))?.value ?? "";
    await act(async () => {
      fireEvent.change(select, { target: { value: option(/^F: Photos/) } });
    });
    await screen.findByText("Ce disque était en E:, il est aujourd'hui en F: : Lumen le suit par son identifiant, pas par sa lettre.");
    expect(screen.getByText(/La projection demande au moins 5 snapshots sur 30 jours \(\d pour l'instant\)\./)).toBeTruthy();
    await act(async () => {
      fireEvent.change(select, { target: { value: option(/^G: \(absent\) Sauvegardes/) } });
    });
    await screen.findByText(/Disque absent \(vu en dernier en G:\)/);
  });

  it("comparaison : top des dossiers, clic vers l'Explorer zoomé ; jamais de faux delta entre deux méthodes", async () => {
    render(<TimelineScreen />);
    await screen.findByRole("combobox", { name: "Série" });
    fireEvent.click(screen.getByRole("button", { name: "3 mois" }));
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Comparer" }));
    });
    const grown = await screen.findByRole("region", { name: "Ont grossi" });
    expect(text(within(grown).getAllByRole("listitem")[0] ?? null)).toMatch(/^Vidéos\\Captures\\2025\+[\d,]+ Go$/);
    expect(text(screen.getByRole("region", { name: "Nouveaux" }))).toContain("SteamLibrary");
    expect(text(screen.getByRole("region", { name: "Disparus" }))).toContain("Téléchargements\\ISO");
    // Un dossier disparu ne s'ouvre pas ; les autres ouvrent l'Explorer sur leur chemin.
    expect(within(screen.getByRole("region", { name: "Disparus" })).queryByRole("button")).toBeNull();
    await act(async () => {
      fireEvent.click(within(grown).getByRole("button", { name: /^Vidéos\\Captures\\2025/ }));
    });
    expect(useStore.getState().screen).toBe("analyse");
    expect(useNav.getState().pending).toBe("C:\\Users\\Arthur\\Vidéos\\Captures\\2025");

    // Même dossier analysé par la MFT : comparaison refusée, sans liste.
    act(() => useStore.setState({ screen: "timeline" }));
    const to = screen.getByRole("combobox", { name: "Au" }) as HTMLSelectElement;
    const mft = [...to.options].find((o) => /MFT/.test(o.textContent ?? ""));
    expect(mft?.parentElement?.getAttribute("label")).toBe("Autres séries (non comparables)");
    fireEvent.change(to, { target: { value: mft?.value } });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Comparer" }));
    });
    await screen.findByText(/ils viennent de méthodes d'analyse différentes \(parcours \/ MFT\)\. Lumen n'affiche pas de faux delta\./);
    expect(screen.queryByRole("region", { name: "Ont grossi" })).toBeNull();
    expect(useTimeline.getState().compare).toMatchObject({ status: "done", value: { comparable: false, diff: null } });
  });

  it("projection : message neutre si la pente est nulle ou négative", () => {
    expect(projectionText(data({ status: "flat", points: 0, bytesPerDay: -3 * GIB, daysToFull: null }), t)).toBe(
      "L'espace utilisé est stable ou en baisse sur les 30 derniers jours, nettoyages de Lumen exclus : pas de projection.",
    );
    expect(projectionText(data({ status: "tooFew", points: 4, bytesPerDay: null, daysToFull: null }), t)).toBe("La projection demande au moins 5 snapshots sur 30 jours (4 pour l'instant).");
    const growing = (days: number | null) => projectionText(data({ status: "growing", points: 12, bytesPerDay: GIB, daysToFull: days }), t).replace(/\u202f|\u00a0/g, " ");
    expect(growing(70)).toBe("Au rythme des 30 derniers jours (7,0 Go par semaine, nettoyages de Lumen exclus), le disque sera plein dans environ 10 semaines.");
    expect(growing(3)).toMatch(/plein dans moins d'une semaine\.$/);
    expect(growing(900)).toMatch(/ne sera pas plein avant plus de 2 ans\.$/);
    expect(growing(null)).toBe("L'espace utilisé augmente de 7,0 Go par semaine sur les 30 derniers jours, nettoyages de Lumen exclus.");
  });

  it("Paramètres : données locales (snapshots et analyses enregistrées), purge après confirmation", async () => {
    useStore.setState({ settingsOpen: true });
    render(<SettingsScreen />);
    const row = (await screen.findByText("Données locales")).closest("div")?.parentElement ?? null;
    await waitFor(() => expect(text(row)).toMatch(/\d+,\d Mo · \d+ snapshots · 5 séries · 2 analyses enregistrées \(40,0 Mo\)/));
    fireEvent.click(screen.getByRole("button", { name: "Purger…" }));
    expect(text(row)).toContain("Les journaux des nettoyages et des déplacements sont gardés");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Supprimer l'historique et les analyses" }));
    });
    await waitFor(() => expect(text(row)).toMatch(/0 snapshot · 0 série · 0 analyse enregistrée/));
    expect(useStore.getState().toast?.text).toBe("Historique et analyses enregistrées purgés");
    expect((screen.getByRole("button", { name: "Purger…" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
