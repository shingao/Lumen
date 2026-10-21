import { ScreenStub } from "./ScreenStub";

export function TimelineScreen() {
  return <ScreenStub title="screen.timeline.title" subtitle="screen.timeline.subtitle" empty="screen.timeline.empty" milestone={7} />;
}

export function DevScreen() {
  return <ScreenStub title="screen.dev.title" subtitle="screen.dev.subtitle" empty="screen.dev.empty" milestone={8} />;
}

export function GamesScreen() {
  return <ScreenStub title="screen.games.title" subtitle="screen.games.subtitle" empty="screen.games.empty" milestone={8} />;
}
