import { useState } from "react";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { Segmented } from "../../components/Segmented";
import { useT } from "../../i18n";
import screens from "../screens/Screens.module.css";
import { DormantTab } from "./DormantTab";
import { DuplicatesTab } from "./DuplicatesTab";
import { LargeTab } from "./LargeTab";
import { SystemTab } from "./SystemTab";
import styles from "./Advanced.module.css";

type CleanupTab = "dormant" | "duplicates" | "large" | "system";

/** Nettoyage avancé : dormants, doublons, gros fichiers (depuis l'analyse), gouffres système. */
export function CleanupScreen() {
  const t = useT();
  const [tab, setTab] = useState<CleanupTab>("dormant");
  const hasScan = useStore((s) => s.scan.summary !== null);
  const go = useStore((s) => s.go);
  const systemOn = useStore((s) => s.settings?.experimentalSystem ?? false);
  const openSettings = useStore((s) => s.setSettingsOpen);
  const tabs: { value: CleanupTab; label: string }[] = [
    { value: "dormant", label: t("screen.cleanup.dormant") },
    { value: "duplicates", label: t("screen.cleanup.duplicates") },
    { value: "large", label: t("screen.cleanup.large") },
    { value: "system", label: t("screen.cleanup.system") },
  ];
  const needsScan = tab !== "system" && !hasScan;
  return (
    <section aria-label={t("screen.cleanup.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.cleanup.title")}</h2>
          <p className={screens.subtitle}>{t("screen.cleanup.subtitle")}</p>
        </div>
        <Segmented<CleanupTab> options={tabs} value={tab} onChange={setTab} label={t("screen.cleanup.tabs")} />
      </div>
      {needsScan ? (
        <div className={screens.empty}>
          <p>{t("adv.noScan")}</p>
          <Button variant="primary" onClick={() => go("analyse")}>
            {t("adv.toAnalyse")}
          </Button>
        </div>
      ) : tab === "system" && !systemOn ? (
        <div className={screens.empty}>
          <p>{t("adv.system.off")}</p>
          <Button variant="primary" onClick={() => openSettings(true)}>
            {t("adv.system.settings")}
          </Button>
        </div>
      ) : (
        <div className={styles.tab}>
          {tab === "dormant" && <DormantTab />}
          {tab === "duplicates" && <DuplicatesTab />}
          {tab === "large" && <LargeTab />}
          {tab === "system" && <SystemTab />}
        </div>
      )}
    </section>
  );
}
