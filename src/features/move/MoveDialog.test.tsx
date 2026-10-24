// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openMove, useMove, useMoves } from "../../app/moves";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import type { MoveCheck, MoveRow } from "../../ipc/types";
import { MoveDialog } from "./MoveDialog";
import { MoveWarnings, MovesList } from "./Moves";

const GB = 1024 ** 3;
const base: MoveCheck = {
  planId: 7,
  fingerprint: "a".repeat(64),
  source: "~\\AppData\\Local\\JetBrains",
  target: "D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\AppData\\Local\\JetBrains",
  drive: "D:",
  drives: [
    { letter: "D:", label: "Données", free: 400 * GB },
    { letter: "E:", label: "Archives", free: 50 * GB },
  ],
  bytes: 10 * GB,
  files: 1200,
  needed: 11 * GB,
  free: 400 * GB,
  review: true,
  refusals: [],
};

describe("Déplacement intelligent", () => {
  beforeEach(() => {
    useStore.setState({ locale: "fr" });
    useMove.setState({ step: "closed" });
  });
  afterEach(() => vi.restoreAllMocks());

  it("refus : dit quel programme tient le dossier, rien ne se lance", async () => {
    vi.spyOn(backend, "moveCheck").mockResolvedValue({ ...base, planId: null, fingerprint: null, review: false, refusals: [{ code: "inUse", message: "ouvert par Discord", apps: ["Discord"] }] });
    const start = vi.spyOn(backend, "moveStart");
    render(<MoveDialog />);
    act(() => openMove(1, 42));
    expect(await screen.findByText("Ouvert par Discord : ferme Discord d'abord.")).toBeTruthy();
    const go = screen.getByRole("button", { name: "Copier et vérifier" }) as HTMLButtonElement;
    expect(go.disabled).toBe(true);
    fireEvent.click(go);
    expect(start).not.toHaveBeenCalled();
  });

  it("dossier d'appli « À vérifier », copie vérifiée, puis suppression seulement après confirmation explicite", async () => {
    const check = vi.spyOn(backend, "moveCheck").mockResolvedValue(base);
    vi.spyOn(backend, "moveStart").mockResolvedValue({ moveId: 3, outcome: "ready", step: null, message: null, refusals: [], source: base.source, target: base.target ?? "", bytes: base.bytes, files: base.files });
    const confirm = vi.spyOn(backend, "moveConfirm").mockResolvedValue({ outcome: "done", message: null, freed: base.bytes });
    vi.spyOn(backend, "moves").mockResolvedValue([]);
    render(<MoveDialog />);
    act(() => openMove(1, 42));
    expect(await screen.findByText(/l'appli peut ne plus se mettre à jour correctement/)).toBeTruthy();
    expect(screen.getByText(/nécessaire : 11,0 Go \(marge de 10 %\)/)).toBeTruthy();
    // Changer de disque refait la vérification.
    fireEvent.click(screen.getByRole("button", { name: /^E: ·/ }));
    await waitFor(() => expect(check).toHaveBeenLastCalledWith(1, 42, "E:"));
    fireEvent.click(await screen.findByRole("button", { name: "Copier et vérifier" }));
    const del = (await screen.findByRole("button", { name: /Supprimer l'original/ })) as HTMLButtonElement;
    expect(screen.getByText(/supprimé définitivement, sans passer par la Corbeille/)).toBeTruthy();
    // Échap ne ferme rien avant la décision sur l'original.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(useMove.getState().step).toBe("ready");
    expect(del.disabled).toBe(true);
    fireEvent.click(del);
    expect(confirm).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText("Je confirme la suppression définitive de l'original"));
    fireEvent.click(del);
    await waitFor(() => expect(confirm).toHaveBeenCalledWith(3, true));
    expect(await screen.findByText("Déplacé : 10,0 Go libérés sur le disque d'origine.")).toBeTruthy();
  });

  it("« Tout remettre comme avant » : retour en arrière, rien n'est supprimé", async () => {
    vi.spyOn(backend, "moveCheck").mockResolvedValue({ ...base, review: false });
    vi.spyOn(backend, "moveStart").mockResolvedValue({ moveId: 4, outcome: "ready", step: null, message: null, refusals: [], source: base.source, target: base.target ?? "", bytes: base.bytes, files: base.files });
    const confirm = vi.spyOn(backend, "moveConfirm").mockResolvedValue({ outcome: "rolledBack", message: null, freed: 0 });
    vi.spyOn(backend, "moves").mockResolvedValue([]);
    render(<MoveDialog />);
    act(() => openMove(1, 42));
    fireEvent.click(await screen.findByRole("button", { name: "Copier et vérifier" }));
    fireEvent.click(await screen.findByRole("button", { name: "Tout remettre comme avant" }));
    await waitFor(() => expect(confirm).toHaveBeenCalledWith(4, false));
    expect(await screen.findByText("Rien n'a changé : tout a été remis comme avant.")).toBeTruthy();
  });

  it("échec en cours de route : retour en arrière annoncé avec l'étape", async () => {
    vi.spyOn(backend, "moveCheck").mockResolvedValue(base);
    vi.spyOn(backend, "moveStart").mockResolvedValue({ moveId: 5, outcome: "rolledBack", step: "verify", message: "contenu différent après la copie : a.bin", refusals: [], source: "", target: "", bytes: 0, files: 0 });
    vi.spyOn(backend, "moves").mockResolvedValue([]);
    render(<MoveDialog />);
    act(() => openMove(1, 42));
    fireEvent.click(await screen.findByRole("button", { name: "Copier et vérifier" }));
    expect(await screen.findByText(/Échec à l'étape « Vérification BLAKE3 » : contenu différent après la copie : a.bin Rien n'a changé/)).toBeTruthy();
  });
});

describe("Suivi des jonctions", () => {
  const row: MoveRow = {
    id: 1,
    startedAt: Date.UTC(2026, 9, 1),
    finishedAt: Date.UTC(2026, 9, 1),
    undoneAt: null,
    source: "C:\\Users\\Arthur\\Videos\\Rushes",
    target: "E:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Videos\\Rushes",
    bytes: 48 * GB,
    files: 300,
    state: "done",
    step: null,
    message: null,
    health: "targetVolumeAbsent",
  };
  beforeEach(() => useMoves.setState({ rows: null, undoing: null }));
  afterEach(() => vi.restoreAllMocks());

  it("avertissement dans l'Analyse si le disque cible est absent", async () => {
    vi.spyOn(backend, "moves").mockResolvedValue([row, { ...row, id: 2, health: "ok" }]);
    render(<MoveWarnings />);
    expect(await screen.findByText("Un dossier déplacé par Lumen est inaccessible")).toBeTruthy();
    expect(screen.getByText(/Rushes : Disque cible absent/)).toBeTruthy();
  });

  it("aucun avertissement quand tout va bien", async () => {
    const list = vi.spyOn(backend, "moves").mockResolvedValue([{ ...row, health: "ok" }]);
    const { container } = render(<MoveWarnings />);
    await waitFor(() => expect(list).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });

  it("« Annuler le déplacement » depuis le journal", async () => {
    vi.spyOn(backend, "moves").mockResolvedValue([{ ...row, health: "ok" }]);
    const undo = vi.spyOn(backend, "moveUndo").mockResolvedValue({ outcome: "done", message: null });
    render(<MovesList />);
    fireEvent.click(await screen.findByRole("button", { name: "Annuler le déplacement" }));
    await waitFor(() => expect(undo).toHaveBeenCalledWith(1));
  });
});
