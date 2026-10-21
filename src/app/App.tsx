import { type JSX, useEffect } from "react";
import { Toast } from "../components/Toast";
import { Segmented } from "../components/Segmented";
import { useT } from "../i18n";
import { Palette } from "../features/palette/Palette";
import { AnalyseScreen } from "../features/screens/AnalyseScreen";
import { CleanDialog } from "../features/cleanup/CleanDialog";
import { JournalScreen } from "../features/journal/JournalScreen";
import { CleanupScreen, DevScreen, GamesScreen, TimelineScreen } from "../features/screens/OtherScreens";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { TitleBar } from "../features/titlebar/TitleBar";
import { screenLabel } from "./commands";
import { matchShortcut } from "./shortcuts";
import { SCREENS, type Screen, useStore } from "./store";
import { toggleTheme } from "./theme";
import styles from "./App.module.css";

const VIEWS: Record<Screen, () => JSX.Element> = {
  analyse: AnalyseScreen,
  timeline: TimelineScreen,
  cleanup: CleanupScreen,
  dev: DevScreen,
  games: GamesScreen,
  journal: JournalScreen,
};

export function App() {
  const t = useT();
  const screen = useStore((s) => s.screen);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const paletteOpen = useStore((s) => s.paletteOpen);
  const go = useStore((s) => s.go);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useStore.getState();
      if (matchShortcut(e, "palette")) s.setPaletteOpen(!s.paletteOpen);
      else if (matchShortcut(e, "settings")) s.setSettingsOpen(!s.settingsOpen);
      else if (matchShortcut(e, "theme")) void toggleTheme();
      else if (e.key === "Escape" && !s.paletteOpen && s.openReco) s.setOpenReco(null);
      else if (e.key === "Escape" && !s.paletteOpen && s.settingsOpen) s.setSettingsOpen(false);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const View = VIEWS[screen];
  const navOptions = SCREENS.map((s) => ({ value: s, label: screenLabel(s, t) }));

  return (
    <div className={styles.app}>
      <TitleBar />
      <div className={styles.scroll}>
        <main className={styles.main}>
          {!settingsOpen && (
            <div className={styles.nav}>
              <Segmented as="nav" label={t("nav.label")} options={navOptions} value={screen} onChange={go} />
            </div>
          )}
          {settingsOpen ? <SettingsScreen /> : <View />}
        </main>
      </div>
      {paletteOpen && <Palette />}
      <CleanDialog />
      <Toast />
    </div>
  );
}
