import { useEffect, useState } from "react";
import { useStore } from "../../app/store";
import { toggleTheme } from "../../app/theme";
import { shortcutLabel } from "../../app/shortcuts";
import {
  CloseIcon,
  GearIcon,
  LogoIcon,
  MaximizeIcon,
  MinimizeIcon,
  MoonIcon,
  RestoreIcon,
  SearchIcon,
  SunIcon,
} from "../../components/icons";
import { appWindow } from "../../ipc/client";
import { useT } from "../../i18n";
import styles from "./TitleBar.module.css";

/** Barre de titre sans décorations natives : la zone vide sert à déplacer la fenêtre. */
export function TitleBar() {
  const t = useT();
  const dark = useStore((s) => s.dark);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const setPaletteOpen = useStore((s) => s.setPaletteOpen);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => appWindow.onMaximizedChange(setMaximized), []);

  return (
    <header
      className={styles.bar}
      data-tauri-drag-region
      onDoubleClick={(e) => {
        if (e.target === e.currentTarget) appWindow.toggleMaximize();
      }}
    >
      <div className={styles.brand} data-tauri-drag-region>
        <LogoIcon />
        <span className={styles.name}>{t("app.name")}</span>
      </div>

      <button type="button" className={styles.search} aria-label={t("titlebar.searchLabel")} onClick={() => setPaletteOpen(true)}>
        <span className={styles.searchIcon}>
          <SearchIcon />
        </span>
        <span className={styles.placeholder}>{t("titlebar.search")}</span>
        <kbd className={styles.kbd}>{shortcutLabel("palette")}</kbd>
      </button>

      <div className={styles.end} data-tauri-drag-region>
        <div className={styles.tools}>
          <button
            type="button"
            className={styles.tool}
            aria-label={dark ? t("titlebar.themeToLight") : t("titlebar.themeToDark")}
            title={dark ? t("titlebar.themeToLight") : t("titlebar.themeToDark")}
            onClick={() => void toggleTheme()}
          >
            {dark ? <SunIcon /> : <MoonIcon />}
          </button>
          <button
            type="button"
            className={styles.tool}
            aria-label={t("titlebar.settings")}
            aria-pressed={settingsOpen}
            title={t("titlebar.settings")}
            onClick={() => setSettingsOpen(!settingsOpen)}
          >
            <GearIcon />
          </button>
        </div>
        <div className={styles.window}>
          <button type="button" className={styles.win} aria-label={t("titlebar.minimize")} onClick={appWindow.minimize}>
            <MinimizeIcon />
          </button>
          <button
            type="button"
            className={styles.win}
            aria-label={maximized ? t("titlebar.restore") : t("titlebar.maximize")}
            onClick={appWindow.toggleMaximize}
          >
            {maximized ? <RestoreIcon /> : <MaximizeIcon />}
          </button>
          <button type="button" className={`${styles.win} ${styles.close}`} aria-label={t("titlebar.close")} onClick={appWindow.close}>
            <CloseIcon />
          </button>
        </div>
      </div>
    </header>
  );
}
