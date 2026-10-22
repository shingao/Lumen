import { useEffect, useState } from "react";
import { ensureScanData, loadGames, openGame, refreshRunning, setShadersOn, useDevGames } from "../../app/devgames";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { sortGames, type GameSort } from "../../core/devgames";
import { formatBytes, formatCount, formatRelative } from "../../core/format";
import { useT, type MessageKey, type Translator } from "../../i18n";
import type { Game, GamesResult, LauncherId } from "../../ipc/types";
import adv from "../advanced/Advanced.module.css";
import { Check, SelectionBar } from "../advanced/FileRows";
import screens from "../screens/Screens.module.css";
import styles from "./Games.module.css";

type GamesTab = "games" | "shaders";

const LAUNCHER_KEYS: Record<LauncherId, MessageKey> = {
  steam: "games.launcher.steam",
  epic: "games.launcher.epic",
  gog: "games.launcher.gog",
  battlenet: "games.launcher.battlenet",
  ea: "games.launcher.ea",
  ubisoft: "games.launcher.ubisoft",
  riot: "games.launcher.riot",
  curseforge: "games.launcher.curseforge",
  minecraft: "games.launcher.minecraft",
  xbox: "games.launcher.xbox",
};

/** Infobulle de « Désinstaller » / « Déplacer » : ce que Lumen ouvre. */
function openTitle(game: Game, action: "uninstall" | "move", launcher: string, t: Translator): string {
  if (game.open === "none") return t("games.open.none", { launcher });
  if (action === "move" && !game.canMove) return t("games.move.none", { launcher });
  if (game.launcher === "xbox") return t("games.xbox.title");
  if (action === "uninstall") return t("games.uninstall.title", { launcher });
  return t(game.launcher === "steam" ? "games.move.steam" : "games.move.title", { launcher });
}

function GameCard({ game }: { game: Game }) {
  const t = useT();
  const cover = useDevGames((s) => s.covers[game.id] ?? null);
  const launcher = t(LAUNCHER_KEYS[game.launcher]);
  const noOpen = game.open === "none";
  return (
    <li className={styles.game} aria-label={game.name}>
      <div className={styles.cover}>{cover ? <img src={cover} alt="" /> : <span aria-hidden="true">{game.name.slice(0, 1)}</span>}</div>
      <div className={styles.body}>
        <span className={styles.name} title={game.name}>
          {game.name}
        </span>
        <span className={styles.meta}>
          {launcher}
          {game.drive ? ` · ${game.drive}` : ""}
        </span>
        <span className={styles.size} title={game.sizeSource === "measured" ? t("games.size.measured") : undefined}>
          {game.size === null ? t("games.size.unknown") : formatBytes(game.size, t.locale)}
          {game.sizeSource === "measured" && <span className={styles.measured}>{t("games.size.measuredShort")}</span>}
        </span>
        <span className={styles.meta}>
          {game.lastPlayed === null ? t("games.lastPlayed.unknown") : t("games.lastPlayed", { when: formatRelative(game.lastPlayed, t.locale) })}
        </span>
        <span className={`${adv.status} ${game.status === "keep" ? styles.keep : ""}`}>
          <StatusDot status={game.status} />
          {t(game.status === "review" ? "status.review" : "status.keep")}
        </span>
        {game.data && (
          <span className={styles.data} title={[t("games.data.hint"), ...game.data.paths].join("\n")}>
            {t("games.data", { size: formatBytes(game.data.size, t.locale) })}
          </span>
        )}
        <span className={styles.actions}>
          <Button variant="link" disabled={noOpen} title={openTitle(game, "uninstall", launcher, t)} onClick={() => void openGame(game.id, "uninstall")}>
            {t("games.uninstall")}
          </Button>
          <Button variant="link" disabled={noOpen || !game.canMove} title={openTitle(game, "move", launcher, t)} onClick={() => void openGame(game.id, "move")}>
            {t("games.move")}
          </Button>
        </span>
      </div>
    </li>
  );
}

function Ignored({ result }: { result: GamesResult }) {
  const t = useT();
  const n = result.ignored.length;
  if (n === 0) return null;
  return (
    <details className={styles.ignored}>
      <summary>{t.plural("games.ignored", n, { n: formatCount(n, t.locale) })}</summary>
      <ul>
        {result.ignored.map((i) => (
          <li key={`${i.launcher}:${i.file}`}>
            <code>{i.file}</code> · {t(`games.reason.${i.reason}`)}
          </li>
        ))}
      </ul>
    </details>
  );
}

function GamesList() {
  const t = useT();
  const games = useDevGames((s) => s.games);
  const openError = useDevGames((s) => s.openError);
  const [sort, setSort] = useState<GameSort>("size");

  useEffect(() => {
    if (games.status === "idle") void loadGames();
  }, [games.status]);

  const result = games.status === "done" ? games.value : null;
  const list = result ? sortGames(result.games, sort, t.locale) : [];
  const total = list.reduce((a, g) => a + (g.size ?? 0), 0);
  const review = list.filter((g) => g.status === "review").length;
  const sorts: { value: GameSort; label: string }[] = [
    { value: "size", label: t("games.sort.size") },
    { value: "lastPlayed", label: t("games.sort.lastPlayed") },
    { value: "name", label: t("games.sort.name") },
  ];
  return (
    <>
      <p className={adv.notice}>{t("games.note")}</p>
      {games.status === "loading" && <p className={adv.hint}>{t("games.loading")}</p>}
      {games.status === "failed" && <p className={adv.warning}>{games.message}</p>}
      {openError && (
        <p className={adv.warning} role="alert">
          {openError}
        </p>
      )}
      {result && (
        <>
          <div className={adv.toolbar}>
            <span className={adv.summaryText}>
              {t.plural("games.summary", list.length, { n: formatCount(list.length, t.locale), size: formatBytes(total, t.locale) })}
              {review > 0 && ` · ${t.plural("games.reviewCount", review, { n: formatCount(review, t.locale) })}`}
            </span>
            <span className={styles.spacer} />
            <span className={adv.toolLabel}>{t("games.sort")}</span>
            <Segmented<GameSort> options={sorts} value={sort} onChange={setSort} label={t("games.sort")} />
          </div>
          {list.length === 0 ? (
            <p className={adv.empty}>{t("games.none")}</p>
          ) : (
            <ul className={styles.grid} aria-label={t("games.list")}>
              {list.map((g) => (
                <GameCard key={g.id} game={g} />
              ))}
            </ul>
          )}
          <Ignored result={result} />
        </>
      )}
    </>
  );
}

/** Caches de shaders : « À vérifier », jamais cochés, refusés si un jeu ou un launcher tourne. */
function ShadersTab() {
  const t = useT();
  const { shaders, shadersOn, running, games } = useDevGames();
  const hasScan = useStore((s) => s.scan.summary !== null);
  const go = useStore((s) => s.go);
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);

  useEffect(() => {
    ensureScanData("shaders");
    void refreshRunning();
    if (games.status === "idle") void loadGames();
  }, [scanId, games.status]);

  const value = shaders.status === "done" ? shaders.value : null;
  const steam = games.status === "done" ? games.value.steamShaderCaches : [];
  const busy = running.length > 0;
  const title = t("shaders.title");
  return (
    <>
      <div className={adv.summary}>
        <span className={adv.status}>
          <StatusDot status="review" />
          {t("status.review")}
        </span>
        <span className={adv.summaryText}>{t("shaders.summary")}</span>
      </div>
      <p className={adv.notice}>{t("shaders.explain")}</p>
      <p className={adv.hint}>{t("shaders.tip")}</p>
      {busy && (
        <p className={adv.warning} role="note">
          {t("shaders.running", { names: running.join(", ") })}
        </p>
      )}
      {!hasScan ? (
        <div className={adv.empty}>
          <p>{t("shaders.noScan")}</p>
          <Button variant="primary" onClick={() => go("analyse")}>
            {t("adv.toAnalyse")}
          </Button>
        </div>
      ) : shaders.status === "loading" ? (
        <p className={adv.hint}>{t("adv.loading")}</p>
      ) : shaders.status === "failed" ? (
        <p className={adv.warning}>{shaders.message}</p>
      ) : value && !value.analysed ? (
        <p className={adv.empty}>{t("shaders.notAnalysed")}</p>
      ) : value && value.caches.length === 0 ? (
        <p className={adv.empty}>{t("shaders.none")}</p>
      ) : (
        value && (
          <section className={adv.group} aria-label={title}>
            <div className={adv.groupHead}>
              <label className={adv.groupLabel}>
                <Check checked={shadersOn} disabled={busy} label={t("shaders.include", { size: formatBytes(value.total, t.locale) })} onChange={setShadersOn} />
                <span className={adv.groupTitle}>{title}</span>
              </label>
              <span className={adv.groupSize}>{formatBytes(value.total, t.locale)}</span>
            </div>
            <ul className={adv.rows}>
              {value.caches.map((c) => (
                <li key={c.path} className={adv.row}>
                  <div className={styles.line}>
                    <code className={adv.rowPath}>{c.path}</code>
                    <span className={adv.rowMeta}>{t.plural("dormant.files", c.files, { n: formatCount(c.files, t.locale) })}</span>
                    <span className={adv.rowSize}>{formatBytes(c.size, t.locale)}</span>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        )
      )}
      {steam.length > 0 && (
        <section className={adv.card} aria-label={t("shaders.steam.title")}>
          <div className={adv.cardHead}>
            <h3 className={adv.cardTitle}>{t("shaders.steam.title")}</h3>
            <span className={adv.tag}>{t("sys.info")}</span>
            <span className={adv.groupSize}>{formatBytes(steam.reduce((a, c) => a + c.size, 0), t.locale)}</span>
          </div>
          <div className={adv.cardBody}>
            <p>{t("shaders.steam.text")}</p>
            {steam.map((c) => (
              <code key={c.path} className={adv.command}>
                {c.path} · {formatBytes(c.size, t.locale)}
              </code>
            ))}
          </div>
        </section>
      )}
      <SelectionBar
        summary={shadersOn && value ? t("shaders.selected", { size: formatBytes(value.total, t.locale) }) : t("shaders.unselected")}
        action={t("adv.clean")}
        disabled={!shadersOn || busy || !value || value.caches.length === 0}
        onAction={() => openRequest({ kind: "shaders" })}
      />
    </>
  );
}

/** Jeux installés (lecture seule chez les launchers) et caches de shaders. */
export function GamesScreen() {
  const t = useT();
  const [tab, setTab] = useState<GamesTab>("games");
  const tabs: { value: GamesTab; label: string }[] = [
    { value: "games", label: t("games.tab.games") },
    { value: "shaders", label: t("games.tab.shaders") },
  ];
  return (
    <section aria-label={t("screen.games.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.games.title")}</h2>
          <p className={screens.subtitle}>{t("screen.games.subtitle")}</p>
        </div>
        <Segmented<GamesTab> options={tabs} value={tab} onChange={setTab} label={t("games.tabs")} />
      </div>
      <div className={adv.tab}>{tab === "games" ? <GamesList /> : <ShadersTab />}</div>
    </section>
  );
}
