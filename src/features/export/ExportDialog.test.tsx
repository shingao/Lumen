// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openExport } from "../../app/report";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import type { ScanSummary, TreeNode } from "../../ipc/types";
import { ExportDialog } from "./ExportDialog";
import { act } from "react";

const node = (path: string, size: number, children: TreeNode[] = []): TreeNode => ({
  id: size,
  name: path.split("\\").pop() ?? path,
  path,
  kind: "dir",
  size,
  logical: size,
  othersCount: null,
  files: 1,
  lastOpened: 0,
  category: "media",
  status: "keep",
  reco: null,
  gain: 0,
  gainStatus: null,
  gainUpTo: false,
  hasChildren: children.length > 0,
  children,
});

const summary = { scanId: 9, rootPath: "C:\\Users\\Arthur", method: "walk", files: 3, dirs: 2, finishedAt: Date.UTC(2026, 9, 24), allocated: 1, volume: "C:" } as ScanSummary;

describe("Export du rapport", () => {
  afterEach(() => vi.restoreAllMocks());

  it("anonymisé par défaut, HTML autonome enregistré dans Téléchargements", async () => {
    useStore.setState({ locale: "fr", target: { kind: "userProfile" }, appInfo: { elevated: false, fastScanAvailable: false, version: "0.9.0", profile: "C:\\Users\\Arthur" } });
    useStore.getState().setScan({ status: "done", summary });
    vi.spyOn(backend, "tree").mockResolvedValue({ root: node("C:\\Users\\Arthur", 300, [node("C:\\Users\\Arthur\\Documents", 200, [node("C:\\Users\\Arthur\\Documents\\Banque Arthur", 200)])]), breadcrumb: [] });
    vi.spyOn(backend, "recommendations").mockResolvedValue([]);
    vi.spyOn(backend, "timelineSeries").mockResolvedValue([]);
    const save = vi.spyOn(backend, "saveReport").mockResolvedValue({ path: "C:\\Users\\Arthur\\Downloads\\Lumen-rapport-x.html", name: "Lumen-rapport-x.html" });
    render(<ExportDialog />);
    act(() => openExport());
    const anon = screen.getByLabelText(/Anonymiser les chemins/) as HTMLInputElement;
    expect(anon.checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Exporter" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    const [format, stamp, content] = save.mock.calls[0] ?? [];
    expect(format).toBe("html");
    expect(stamp).toMatch(/^\d{4}-\d\d-\d\d-\d{4}$/);
    expect(content).toContain("<!DOCTYPE html>");
    expect(content).not.toContain("Arthur");
    expect(content).not.toMatch(/https?:\/\//);
    expect(await screen.findByText("Lumen-rapport-x.html")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Afficher dans l'Explorateur" })).toBeTruthy();
  });

  it("CSV sans anonymisation si on décoche", async () => {
    useStore.getState().setScan({ status: "done", summary });
    vi.spyOn(backend, "tree").mockResolvedValue({ root: node("C:\\Users\\Arthur", 300, [node("C:\\Users\\Arthur\\Documents", 200)]), breadcrumb: [] });
    vi.spyOn(backend, "recommendations").mockResolvedValue([]);
    vi.spyOn(backend, "timelineSeries").mockResolvedValue([]);
    const save = vi.spyOn(backend, "saveReport").mockResolvedValue({ path: "x", name: "x.csv" });
    render(<ExportDialog />);
    act(() => openExport());
    fireEvent.click(screen.getByRole("button", { name: "CSV" }));
    fireEvent.click(screen.getByLabelText(/Anonymiser les chemins/));
    fireEvent.click(screen.getByRole("button", { name: "Exporter" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    const [format, , content] = save.mock.calls[0] ?? [];
    expect(format).toBe("csv");
    expect(content).toContain("C:\\Users\\Arthur\\Documents");
  });
});
