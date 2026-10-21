import type { ReactNode } from "react";
import { useStore } from "../../app/store";
import { updateSettings } from "../../app/theme";
import { Button } from "../../components/Button";
import { ChevronLeftIcon } from "../../components/icons";
import { Segmented } from "../../components/Segmented";
import { Toggle } from "../../components/Toggle";
import { formatPercent } from "../../core/format";
import { currentT, useT } from "../../i18n";
import type { LanguagePref, Settings, ThemePref } from "../../ipc/types";
import pkg from "../../../package.json";
import styles from "./SettingsScreen.module.css";

function Row({ label, hint, mono, children }: { label: string; hint?: string; mono?: boolean; children?: ReactNode }) {
  return (
    <div className={styles.row}>
      <div className={styles.rowText}>
        <span className={styles.rowLabel}>{label}</span>
        {hint && <span className={mono ? `${styles.rowHint} ${styles.mono}` : styles.rowHint}>{hint}</span>}
      </div>
      <div className={styles.rowControl}>{children}</div>
    </div>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className={styles.group} aria-label={title}>
      <h3 className={styles.groupTitle}>{title}</h3>
      <div className={styles.rows}>{children}</div>
    </section>
  );
}

const DORMANT = [3, 6, 12, 24] as const;
const DUP_SIZES = [1, 10, 100] as const;
const THRESHOLDS = [5, 10, 15, 20] as const;

/** Paramètres : même conteneur que les écrans, une ligne par réglage. */
export function SettingsScreen() {
  const t = useT();
  const settings = useStore((s) => s.settings);
  const close = useStore((s) => s.setSettingsOpen);
  const showToast = useStore((s) => s.showToast);
  if (!settings) return null;
  const set = (patch: Partial<Settings>) => void updateSettings(patch);
  const later = (n: number) => () => showToast(currentT()("preview.later", { n }));

  return (
    <section aria-label={t("settings.title")} className={styles.panel}>
      <div className={styles.header}>
        <button type="button" className={styles.back} onClick={() => close(false)}>
          <ChevronLeftIcon />
          {t("settings.back")}
        </button>
        <h2 className={styles.title}>{t("settings.title")}</h2>
        <span className={styles.version}>{t("settings.version", { v: pkg.version })}</span>
      </div>

      <div className={styles.columns}>
        <div className={styles.column}>
          <Group title={t("settings.appearance")}>
            <Row label={t("settings.theme")} hint={t("settings.themeHint")}>
              <Segmented<ThemePref>
                label={t("settings.theme")}
                value={settings.theme}
                onChange={(theme) => set({ theme })}
                options={[
                  { value: "system", label: t("settings.theme.system") },
                  { value: "light", label: t("settings.theme.light") },
                  { value: "dark", label: t("settings.theme.dark") },
                ]}
              />
            </Row>
            <Row label={t("settings.language")} hint={t("settings.languageHint")}>
              <Segmented<LanguagePref>
                label={t("settings.language")}
                value={settings.language}
                onChange={(language) => set({ language })}
                options={[
                  { value: "system", label: t("settings.language.system") },
                  { value: "fr", label: t("settings.language.fr") },
                  { value: "en", label: t("settings.language.en") },
                ]}
              />
            </Row>
          </Group>

          <Group title={t("settings.scan")}>
            <Row label={t("settings.excluded")} hint={t("settings.excludedNone")}>
              <Button variant="link" onClick={later(3)}>
                {t("settings.excludedAdd")}
              </Button>
            </Row>
            <Row label={t("settings.dormant")} hint={t("settings.dormantHint")}>
              <Segmented
                label={t("settings.dormant")}
                value={settings.dormantMonths}
                onChange={(dormantMonths) => set({ dormantMonths })}
                options={DORMANT.map((n) => ({ value: n, label: t("settings.months", { n }) }))}
              />
            </Row>
            <Row label={t("settings.duplicates")} hint={t("settings.duplicatesHint")}>
              <Segmented
                label={t("settings.duplicates")}
                value={settings.duplicateMinMb}
                onChange={(duplicateMinMb) => set({ duplicateMinMb })}
                options={DUP_SIZES.map((n) => ({ value: n, label: `${n} ${t.locale === "fr" ? "Mo" : "MB"}` }))}
              />
            </Row>
          </Group>

          <Group title={t("settings.cleaning")}>
            <Row label={t("settings.deleteMode")} hint={t("settings.deleteModeHint")}>
              <span className={styles.value}>{t("settings.deleteModeValue")}</span>
            </Row>
            <Row label={t("settings.permanent")} hint={t("settings.permanentHint")}>
              <Toggle label={t("settings.permanent")} checked={settings.permanentDelete} onChange={(permanentDelete) => set({ permanentDelete })} />
            </Row>
          </Group>
        </div>

        <div className={styles.column}>
          <Group title={t("settings.background")}>
            <Row label={t("settings.snapshot")} hint={t("settings.snapshotHint")}>
              <Toggle label={t("settings.snapshot")} checked={settings.dailySnapshot} onChange={(dailySnapshot) => set({ dailySnapshot })} />
            </Row>
            <Row label={t("settings.tray")} hint={t("settings.trayHint")}>
              <Toggle label={t("settings.tray")} checked={settings.trayIcon} onChange={(trayIcon) => set({ trayIcon })} />
            </Row>
            <Row label={t("settings.alert")} hint={t("settings.alertHint")}>
              <Toggle label={t("settings.alert")} checked={settings.lowSpaceAlert} onChange={(lowSpaceAlert) => set({ lowSpaceAlert })} />
            </Row>
            <Row label={t("settings.threshold")}>
              <Segmented
                label={t("settings.threshold")}
                value={settings.lowSpaceThreshold}
                onChange={(lowSpaceThreshold) => set({ lowSpaceThreshold })}
                options={THRESHOLDS.map((n) => ({ value: n, label: formatPercent(n / 100, t.locale) }))}
              />
            </Row>
          </Group>

          <Group title={t("settings.windows")}>
            <Row label={t("settings.explorerMenu")} hint={t("settings.explorerMenuHint")}>
              <Toggle label={t("settings.explorerMenu")} checked={settings.explorerMenu} onChange={(explorerMenu) => set({ explorerMenu })} />
            </Row>
          </Group>

          <Group title={t("settings.experimental")}>
            <Row label={t("settings.experimentalSystem")} hint={t("settings.experimentalSystemHint")}>
              <Toggle
                label={t("settings.experimentalSystem")}
                checked={settings.experimentalSystem}
                onChange={(experimentalSystem) => set({ experimentalSystem })}
              />
            </Row>
          </Group>

          <Group title={t("settings.help")}>
            <Row label={t("settings.tutorial")} hint={t("settings.tutorialHint")}>
              <Button variant="link" onClick={later(10)}>
                {t("settings.tutorialRun")}
              </Button>
            </Row>
            <Row label={t("settings.logs")} hint={settings.logsFolder} mono>
              <Button variant="link" onClick={later(5)}>
                {t("settings.logsOpen")}
              </Button>
            </Row>
          </Group>
        </div>
      </div>
    </section>
  );
}
