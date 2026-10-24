// Commandes nommées, utilisées par la palette (Ctrl K) et les raccourcis globaux.

import { openCleanup } from "./cleaning";
import { copyText } from "./clipboard";
import { backend } from "../ipc/client";
import { currentT, type Translator } from "../i18n";
import { SCREENS, type Screen, useStore } from "./store";
import { updateSettings } from "./theme";
import { startScan } from "./scanning";
import { openMove } from "./moves";
import { openExport } from "./report";

export interface Command {
  id: string;
  label: string;
  hint?: string;
  run(): void;
}

const SCREEN_KEYS = {
  analyse: "nav.analyse",
  timeline: "nav.timeline",
  cleanup: "nav.cleanup",
  dev: "nav.dev",
  games: "nav.games",
  journal: "nav.journal",
} as const satisfies Record<Screen, string>;

export function screenLabel(screen: Screen, t: Translator = currentT()): string {
  return t(SCREEN_KEYS[screen]);
}

export function commandList(): Command[] {
  const t = currentT();
  const store = useStore.getState();
  return [
    { id: "scan", label: t("cmd.scan"), run: () => void startScan(useStore.getState().target) },
    { id: "theme-dark", label: t("cmd.themeDark"), run: () => void updateSettings({ theme: "dark" }) },
    { id: "theme-light", label: t("cmd.themeLight"), run: () => void updateSettings({ theme: "light" }) },
    { id: "theme-system", label: t("cmd.themeSystem"), run: () => void updateSettings({ theme: "system" }) },
    ...SCREENS.map((screen) => ({
      id: `go-${screen}`,
      label: t("cmd.goto", { screen: screenLabel(screen) }),
      run: () => store.go(screen),
    })),
    { id: "settings", label: t("cmd.settings"), run: () => store.setSettingsOpen(true) },
    ...(store.scan.summary ? [{ id: "export", label: t("cmd.export"), run: openExport }] : []),
    ...(store.scan.summary && store.node !== null
      ? [
          {
            id: "move",
            label: t("cmd.move"),
            run: () => {
              const { scan, node } = useStore.getState();
              if (scan.summary && node !== null) openMove(scan.summary.scanId, node);
            },
          },
        ]
      : []),
    {
      id: "clean",
      label: t("cmd.clean"),
      run: () => {
        store.go("analyse");
        openCleanup(useStore.getState().checked);
      },
    },
    {
      id: "copy-script",
      label: t("cmd.copyScript"),
      run: () => {
        const { checked, scan } = useStore.getState();
        if (!scan.summary) return;
        void backend
          .script(scan.summary.scanId, checked)
          .then(copyText)
          .then((ok) => ok && store.showToast(t("preview.scriptCopied")));
      },
    },
    ...(["category", "cleanup", "age"] as const).map((mode) => ({
      id: `color-${mode}`,
      label: t("cmd.colorBy", { mode: t(`explorer.mode.${mode}`).toLowerCase() }),
      run: () => {
        store.go("analyse");
        store.setColorMode(mode);
      },
    })),
  ];
}
