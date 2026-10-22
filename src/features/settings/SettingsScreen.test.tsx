// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "../../app/store";
import { backend } from "../../ipc/client";
import { DEFAULT_SETTINGS } from "../../ipc/mock";
import { SettingsScreen } from "./SettingsScreen";

const text = (el: Element | null) => (el?.textContent ?? "").replace(/[\u202f\u00a0]/g, " ");

async function scanned(target: Parameters<typeof backend.startScan>[0]) {
  const id = await backend.startScan(target, "auto");
  await new Promise<void>((resolve) => {
    const off = backend.onScanEvent((e) => {
      if (e.type === "done" && e.summary.scanId === id) {
        off();
        resolve();
      }
    });
  });
}

describe("Paramètres", () => {
  beforeEach(() => {
    useStore.setState({ locale: "fr", settings: { ...DEFAULT_SETTINGS } });
  });

  it("mémoire des analyses gardées : consommation, détail par disque et limite", async () => {
    await scanned({ kind: "userProfile" });
    await scanned({ kind: "volume", letter: "C:" });
    render(<SettingsScreen />);
    const size = await screen.findByText(/sur 600 Mo/);
    expect(text(size)).toBe("508 Mo sur 600 Mo · 2 analyses");
    expect(text(screen.getByText(/^C: /))).toBe("C: 412 Mo · dossier utilisateur 96,0 Mo");
    expect(screen.getByText(/la moins récemment utilisée est oubliée/)).toBeTruthy();
    const meter = screen.getByRole("meter", { name: "Analyses gardées en mémoire" });
    expect(meter.getAttribute("aria-valuemax")).toBe(String(600 * 1024 * 1024));
  });
});
