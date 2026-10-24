// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { initScanning } from "../../app/scanning";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { ScanStats } from "./ScanPanel";

describe("Analyse enregistrée", () => {
  afterEach(() => {
    localStorage.removeItem("lumen-dev-restored");
    vi.restoreAllMocks();
  });

  it("au lancement : dernière analyse affichée tout de suite, sans réanalyser ; « Réanalyser » à la demande", async () => {
    localStorage.setItem("lumen-dev-restored", "1");
    useStore.setState({ locale: "fr", target: { kind: "userProfile" }, scan: { id: null, status: "idle", method: "auto", progress: null, summary: null, message: null } });
    const start = vi.spyOn(backend, "startScan");
    await initScanning();
    const { scan } = useStore.getState();
    expect(scan.status).toBe("done");
    expect(scan.summary?.restored).toBe(true);
    expect(start).not.toHaveBeenCalled();
    render(<ScanStats />);
    expect(screen.getByText(/analysé (aujourd'hui|hier), \d\d:\d\d/)).toBeTruthy();
    expect(screen.getByText(/Dernière analyse enregistrée/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Réanalyser" }));
    expect(start).toHaveBeenCalledWith({ kind: "userProfile" }, "auto");
  });

  it("sans analyse enregistrée : analyse lancée comme avant", async () => {
    useStore.setState({ target: { kind: "volume", letter: "D:" }, scan: { id: null, status: "idle", method: "auto", progress: null, summary: null, message: null } });
    const start = vi.spyOn(backend, "startScan");
    await initScanning();
    expect(start).toHaveBeenCalledWith({ kind: "volume", letter: "D:" }, "auto");
  });
});
