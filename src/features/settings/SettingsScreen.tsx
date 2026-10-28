import { type ReactNode, useEffect, useState } from "react";
import { useStore } from "../../app/store";
import { updateSettings } from "../../app/theme";
import { dailyTick } from "../../app/timeline";
import { backend } from "../../ipc/client";
import { Button } from "../../components/Button";
import { ChevronLeftIcon } from "../../components/icons";
import { Segmented } from "../../components/Segmented";
import { Toggle } from "../../components/Toggle";
import { formatBytes, formatCount, formatPercent } from "../../core/format";
import { currentT, useT } from "../../i18n";
import type { DatabaseInfo, LanguagePref, ScanCacheInfo, Settings, ThemePref } from "../../ipc/types";
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

/** Historique de la Timeline : taille de la base, purge après confirmation. */
function DatabaseRow() {
  const t = useT();
  const showToast = useStore((s) => s.showToast);
  const [info, setInfo] = useState<DatabaseInfo | null>(null);
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    void backend.database().then(setInfo, () => setInfo(null));
  }, []);
  const purge = () =>
    void backend.purgeSnapshots().then((i) => {
      setInfo(i);
      setConfirming(false);
      showToast(currentT()("settings.databasePurged"));
    });
  const size = info
    ? t("settings.databaseSize", {
        size: formatBytes(info.bytes, t.locale),
        snapshots: t.plural("timeline.snapshots", info.snapshots, { n: formatCount(info.snapshots, t.locale) }),
        series: t.plural("settings.database.series", info.series, { n: info.series }),
        saved: t.plural("settings.database.saved", info.savedScans, { n: info.savedScans, size: formatBytes(info.savedBytes, t.locale) }),
      })
    : "";
  return (
    <div className={styles.row}>
      <div className={styles.rowText}>
        <span className={styles.rowLabel}>{t("settings.database")}</span>
        {info && <span className={`${styles.rowHint} ${styles.mono}`}>{size}</span>}
        <span className={styles.rowHint}>{t(confirming ? "settings.databasePurgeNote" : "settings.databaseHint")}</span>
      </div>
      <div className={styles.rowControl}>
        {confirming ? (
          <span className={styles.actions}>
            <Button variant="link" onClick={() => setConfirming(false)}>
              {t("settings.databasePurgeCancel")}
            </Button>
            <Button variant="primary" onClick={purge}>
              {t("settings.databasePurgeConfirm")}
            </Button>
          </span>
        ) : (
          <Button variant="link" disabled={!info || (info.snapshots === 0 && info.savedScans === 0)} onClick={() => setConfirming(true)}>
            {t("settings.databasePurge")}
          </Button>
        )}
      </div>
    </div>
  );
}

/** Analyses gardées en mémoire pendant la session : consommation et limite. */
function CacheRow() {
  const t = useT();
  const [info, setInfo] = useState<ScanCacheInfo | null>(null);
  useEffect(() => {
    void backend.scanCacheInfo().then(setInfo, () => setInfo(null));
  }, []);
  const n = info?.entries.length ?? 0;
  const detail = info?.entries
    .map((e) => `${e.target === "userProfile" ? t("settings.cache.profile") : e.target} ${formatBytes(e.bytes, t.locale)}`)
    .join(" · ");
  return (
    <div className={styles.row}>
      <div className={styles.rowText}>
        <span className={styles.rowLabel}>{t("settings.cache")}</span>
        {info && (
          <span className={`${styles.rowHint} ${styles.mono}`}>
            {t("settings.cacheSize", { size: formatBytes(info.total, t.locale), limit: formatBytes(info.limit, t.locale), count: t.plural("settings.cache.count", n, { n }) })}
          </span>
        )}
        {detail && <span className={`${styles.rowHint} ${styles.mono}`}>{detail}</span>}
        <span className={styles.rowHint}>{t("settings.cacheHint", { limit: formatBytes(info?.limit ?? 600 * 1024 * 1024, t.locale) })}</span>
      </div>
      {info && (
        <div className={styles.rowControl}>
          <span className={styles.meter} role="meter" aria-label={t("settings.cache")} aria-valuemin={0} aria-valuemax={info.limit} aria-valuenow={info.total}>
            <span style={{ width: `${Math.round(Math.min(1, info.total / Math.max(1, info.limit)) * 100)}%` }} />
          </span>
        </div>
      )}
    </div>
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
            <CacheRow />
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
              <Toggle label={t("settings.snapshot")} checked={settings.dailySnapshot} onChange={(dailySnapshot) => void updateSettings({ dailySnapshot }).then(dailyTick)} />
            </Row>
            <DatabaseRow />
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
            <Row label={t("settings.experimentalWsl")} hint={t("settings.experimentalWslHint")}>
              <Toggle label={t("settings.experimentalWsl")} checked={settings.experimentalWsl} onChange={(experimentalWsl) => set({ experimentalWsl })} />
            </Row>
            <Row label={t("settings.experimentalMove")} hint={t("settings.experimentalMoveHint")}>
              <Toggle label={t("settings.experimentalMove")} checked={settings.experimentalMove} onChange={(experimentalMove) => set({ experimentalMove })} />
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
