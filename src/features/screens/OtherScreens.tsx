import { useState } from "react";
import { Segmented } from "../../components/Segmented";
import { useT } from "../../i18n";
import { ScreenStub } from "./ScreenStub";

export function TimelineScreen() {
  return <ScreenStub title="screen.timeline.title" subtitle="screen.timeline.subtitle" empty="screen.timeline.empty" milestone={7} />;
}

type CleanupTab = "dormant" | "duplicates" | "large" | "system";

export function CleanupScreen() {
  const t = useT();
  const [tab, setTab] = useState<CleanupTab>("dormant");
  const tabs: { value: CleanupTab; label: string }[] = [
    { value: "dormant", label: t("screen.cleanup.dormant") },
    { value: "duplicates", label: t("screen.cleanup.duplicates") },
    { value: "large", label: t("screen.cleanup.large") },
    { value: "system", label: t("screen.cleanup.system") },
  ];
  return (
    <ScreenStub
      title="screen.cleanup.title"
      subtitle="screen.cleanup.subtitle"
      empty="screen.cleanup.empty"
      milestone={6}
      tools={<Segmented<CleanupTab> options={tabs} value={tab} onChange={setTab} label={t("screen.cleanup.tabs")} />}
    />
  );
}

export function DevScreen() {
  return <ScreenStub title="screen.dev.title" subtitle="screen.dev.subtitle" empty="screen.dev.empty" milestone={8} />;
}

export function GamesScreen() {
  return <ScreenStub title="screen.games.title" subtitle="screen.games.subtitle" empty="screen.games.empty" milestone={8} />;
}

export function JournalScreen() {
  return <ScreenStub title="screen.journal.title" subtitle="screen.journal.subtitle" empty="screen.journal.empty" milestone={5} />;
}
