import { useEffect, useState } from "react";
import { useStore } from "../../app/store";
import { dailyTick, loadTimeline, sameSeries, selectSeries, setRange, useTimeline } from "../../app/timeline";
import { Segmented } from "../../components/Segmented";
import { formatBytes, formatCount, formatDateTime, formatDelta } from "../../core/format";
import { inRange, rangeStart, type TimelineRange } from "../../core/timeline";
import { type Translator, useT } from "../../i18n";
import type { TimelineData, TimelineSeries } from "../../ipc/types";
import screens from "../screens/Screens.module.css";
import { Chart } from "./Chart";
import { Compare } from "./Compare";
import { dailyText, projectionText, seriesLabel } from "./labels";
import styles from "./Timeline.module.css";

const RANGES: TimelineRange[] = ["7d", "30d", "3m", "all"];
const DAY = 86_400_000;

function Summary({ data, t }: { data: TimelineData; t: Translator }) {
  const last = data.points[data.points.length - 1];
  return (
    <div className={styles.summary}>
      <div className={styles.cell}>
        <span className={styles.cellLabel}>{t("timeline.used")}</span>
        <span className={styles.cellValue}>{last ? formatBytes(last.used, t.locale) : "—"}</span>
        <span className={styles.cellSub}>{last ? formatDateTime(last.at, t.locale) : ""}</span>
      </div>
      <div className={styles.cell}>
        <span className={styles.cellLabel}>{t("timeline.trend")}</span>
        <span className={styles.cellValue}>{data.trend7d === null ? "—" : formatDelta(data.trend7d, t.locale)}</span>
        <span className={styles.cellSub}>{data.trend7d === null ? t("timeline.trendNone") : t.plural("timeline.snapshots", data.points.length, { n: formatCount(data.points.length, t.locale) })}</span>
      </div>
      <div className={styles.cell}>
        <span className={styles.cellLabel}>{t("timeline.free")}</span>
        <span className={styles.cellValue}>{data.free === null ? "—" : formatBytes(data.free, t.locale)}</span>
        <span className={styles.cellSub}>{data.total === null ? "" : `/ ${formatBytes(data.total, t.locale)}`}</span>
      </div>
    </div>
  );
}

/** Timeline : courbe de l'espace utilisé par série, repères des nettoyages, projection, comparaison. */
export function TimelineScreen() {
  const t = useT();
  const { series, selected, range, data, from, to, daily } = useTimeline();
  const dailyOn = useStore((s) => s.settings?.dailySnapshot ?? true);
  const [now] = useState(() => Date.now());

  useEffect(() => {
    void loadTimeline();
    // État du snapshot quotidien (il ne se lance que s'il est dû).
    if (useTimeline.getState().daily === null) void dailyTick();
  }, []);

  const list = series.status === "done" ? series.value : [];
  const current = list.find((s) => sameSeries(s, selected)) ?? null;
  const value = data.status === "done" ? data.value : null;
  const start = rangeStart(range, now) ?? Math.min(value?.points[0]?.at ?? now - 30 * DAY, now - DAY);
  const points = value ? inRange(value.points, range, now) : [];
  const markers = value ? inRange(value.markers, range, now) : [];
  // Même dossier, autre méthode : signalé ; même disque, autre racine ou méthode : comparaison refusée.
  const others = current ? list.filter((s) => s.volumeId === current.volumeId && !sameSeries(s, current)) : [];
  const otherMethods = current ? others.filter((s) => s.root.toLowerCase() === current.root.toLowerCase()).length : 0;
  const key = (s: TimelineSeries) => `${s.volumeId}|${s.root}|${s.method}`;

  return (
    <section aria-label={t("screen.timeline.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.timeline.title")}</h2>
          <p className={screens.subtitle}>{t("screen.timeline.subtitle")}</p>
        </div>
        <Segmented<TimelineRange> label={t("timeline.range")} value={range} onChange={setRange} options={RANGES.map((r) => ({ value: r, label: t(`timeline.range.${r}`) }))} />
      </div>

      {series.status === "failed" && <p className={styles.warning}>{series.message}</p>}
      {series.status === "done" && list.length === 0 && (
        <div className={screens.empty}>
          <p>{t("screen.timeline.empty")}</p>
        </div>
      )}

      {current && (
        <>
          <div className={styles.toolbar}>
            <label className={styles.field}>
              <span>{t("timeline.series")}</span>
              <select
                className={styles.select}
                value={key(current)}
                onChange={(e) => {
                  const s = list.find((x) => key(x) === e.target.value);
                  if (s) void selectSeries({ volumeId: s.volumeId, root: s.root, method: s.method });
                }}
              >
                {list.map((s) => (
                  <option key={key(s)} value={key(s)}>
                    {seriesLabel(s, t)} · {t.plural("timeline.snapshots", s.count, { n: s.count })}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {current.letter === null && <p className={styles.notice}>{t("timeline.absent", { letter: current.lastLetter })}</p>}
          {current.letter !== null && current.letter !== current.lastLetter && <p className={styles.notice}>{t("timeline.letterChanged", { old: current.lastLetter, now: current.letter })}</p>}
          {otherMethods > 0 && <p className={styles.notice}>{t.plural("timeline.others", otherMethods, { n: otherMethods })}</p>}

          {value && (
            <>
              <Summary data={value} t={t} />
              <div className={styles.card}>
                {points.length === 0 ? (
                  <p className={styles.chartEmpty}>{t("timeline.noPoints")}</p>
                ) : (
                  <Chart
                    points={points}
                    markers={markers}
                    start={start}
                    end={now}
                    from={from ?? points[0]?.id ?? null}
                    to={to ?? points[points.length - 1]?.id ?? null}
                    label={t("timeline.chart", { series: seriesLabel(current, t), from: formatDateTime(start, t.locale), to: formatDateTime(now, t.locale) })}
                  />
                )}
                <p className={styles.projection} data-status={value.projection.status}>
                  {projectionText(value, t)}
                </p>
              </div>
              <p className={styles.hint}>{dailyText(daily, dailyOn, t, now).join(" ")}</p>
              {points.length > 0 && <Compare points={points} others={others} />}
            </>
          )}
        </>
      )}
    </section>
  );
}
