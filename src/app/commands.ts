// Commandes nommées, utilisées par la palette (Ctrl K) et les raccourcis globaux.

import { copyText } from "./clipboard";
import { backend } from "../ipc/client";
import { currentT, type Translator } from "../i18n";
import { SCREENS, type Screen, useStore } from "./store";
import { updateSettings } from "./theme";

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
  const later = (n: number) => () => store.showToast(t("preview.later", { n }));
  return [
    { id: "scan", label: t("cmd.scan"), hint: t("screen.milestone", { n: 3 }), run: later(3) },
    { id: "theme-dark", label: t("cmd.themeDark"), run: () => void updateSettings({ theme: "dark" }) },
    { id: "theme-light", label: t("cmd.themeLight"), run: () => void updateSettings({ theme: "light" }) },
    { id: "theme-system", label: t("cmd.themeSystem"), run: () => void updateSettings({ theme: "system" }) },
    ...SCREENS.map((screen) => ({
      id: `go-${screen}`,
      label: t("cmd.goto", { screen: screenLabel(screen) }),
      run: () => store.go(screen),
    })),
    { id: "settings", label: t("cmd.settings"), run: () => store.setSettingsOpen(true) },
    {
      id: "copy-script",
      label: t("cmd.copyScript"),
      run: () => {
        const { checked } = useStore.getState();
        void backend
          .script(checked, true)
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
