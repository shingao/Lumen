import { type JSX, useEffect } from "react";
import { Toast } from "../components/Toast";
import { Segmented } from "../components/Segmented";
import { useT } from "../i18n";
import { Palette } from "../features/palette/Palette";
import { AnalyseScreen } from "../features/screens/AnalyseScreen";
import { CleanDialog } from "../features/cleanup/CleanDialog";
import { ExportDialog } from "../features/export/ExportDialog";
import { MoveDialog } from "../features/move/MoveDialog";
import { JournalScreen } from "../features/journal/JournalScreen";
import { CleanupScreen } from "../features/advanced/CleanupScreen";
import { DevScreen } from "../features/dev/DevScreen";
import { GamesScreen } from "../features/games/GamesScreen";
import { TimelineScreen } from "../features/timeline/TimelineScreen";
import { SettingsScreen } from "../features/settings/SettingsScreen";
import { TitleBar } from "../features/titlebar/TitleBar";
import { Onboarding } from "../features/onboarding/Onboarding";
import { TipAnchor } from "../features/tips/TipBubble";
import { useOnboarding } from "./onboarding";
import { screenLabel } from "./commands";
import { backend } from "../ipc/client";
import { goBack, goForward } from "./navigation";
import { dailyTick, loadTimeline, useTimeline } from "./timeline";
import { matchShortcut, navigationButton, navigationKey } from "./shortcuts";
import { SCREENS, type Screen, useStore } from "./store";
import { toggleTheme } from "./theme";
import styles from "./App.module.css";

/** Premier snapshot quotidien demandé une minute après le démarrage, puis toutes les 30 min. */
const DAILY_FIRST_MS = 60_000;
const DAILY_EVERY_MS = 30 * 60_000;

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
  const onboarding = useOnboarding((s) => s.open);

  useEffect(() => {
    // Précédent / suivant : seulement sur l'écran Analyse, sans fenêtre ouverte par-dessus.
    const canNavigate = () => {
      const s = useStore.getState();
      return s.screen === "analyse" && !s.settingsOpen && !s.paletteOpen && s.clean.step === "closed" && s.openReco === null;
    };
    const navigate = (to: "back" | "forward" | null) => {
      if (to === null || !canNavigate()) return false;
      if (to === "back") goBack();
      else goForward();
      return true;
    };
    const onMouse = (e: MouseEvent) => {
      if (navigate(navigationButton(e))) e.preventDefault();
    };
    const onKey = (e: KeyboardEvent) => {
      // Tutoriel ouvert : il gère seul son clavier (flèches, Échap, Tab).
      if (useOnboarding.getState().open) return;
      const s = useStore.getState();
      if (navigate(navigationKey(e))) {
        e.preventDefault();
        return;
      }
      if (matchShortcut(e, "palette")) s.setPaletteOpen(!s.paletteOpen);
      else if (matchShortcut(e, "settings")) s.setSettingsOpen(!s.settingsOpen);
      else if (matchShortcut(e, "theme")) void toggleTheme();
      else if (e.key === "Escape" && !s.paletteOpen && s.openReco) s.setOpenReco(null);
      else if (e.key === "Escape" && !s.paletteOpen && s.settingsOpen) s.setSettingsOpen(false);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mouseup", onMouse);
    };
  }, []);

  useEffect(() => {
    // Snapshot quotidien seulement tant que l'appli tourne : ni service, ni tâche planifiée.
    const first = window.setTimeout(() => void dailyTick(), DAILY_FIRST_MS);
    const every = window.setInterval(() => void dailyTick(), DAILY_EVERY_MS);
    const off = backend.onTimelineChanged(() => {
      if (useTimeline.getState().series.status !== "idle") void loadTimeline();
    });
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
      off();
    };
  }, []);

  const View = VIEWS[screen];
  const navOptions = SCREENS.map((s) => ({ value: s, label: screenLabel(s, t) }));

  return (
    <div className={styles.app}>
      <TitleBar />
      <div className={styles.scroll}>
        {onboarding ? (
          <Onboarding />
        ) : (
          <main className={styles.main}>
            {!settingsOpen && (
              <div className={styles.nav}>
                <TipAnchor>
                  <Segmented as="nav" label={t("nav.label")} options={navOptions} value={screen} onChange={go} />
                </TipAnchor>
              </div>
            )}
            {settingsOpen ? <SettingsScreen /> : <View />}
          </main>
        )}
      </div>
      {paletteOpen && <Palette />}
      <CleanDialog />
      <MoveDialog />
      <ExportDialog />
      <Toast />
    </div>
  );
}
