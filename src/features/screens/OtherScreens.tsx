import { ScreenStub } from "./ScreenStub";

export function DevScreen() {
  return <ScreenStub title="screen.dev.title" subtitle="screen.dev.subtitle" empty="screen.dev.empty" milestone={8} />;
}

export function GamesScreen() {
  return <ScreenStub title="screen.games.title" subtitle="screen.games.subtitle" empty="screen.games.empty" milestone={8} />;
}
