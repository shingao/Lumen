import { useState } from "react";
import { useSize } from "../../app/useSize";
import { formatBytes, formatDateTime, formatDay, formatDelta } from "../../core/format";
import { nearest, scaleOf } from "../../core/timeline";
import { useT } from "../../i18n";
import type { TimelineMarker, TimelinePoint } from "../../ipc/types";
import pop from "../../components/Popover.module.css";
import styles from "./Timeline.module.css";

/** Marges du tracé dans le cadre : graduations à gauche, dates en bas, repères en haut. */
const PAD = { left: 64, right: 18, top: 34, bottom: 28 };
/** Hauteur d'une rangée de repères (deux repères proches passent sur deux rangées). */
const MARKER_ROW = 24;
const MARKER_MIN_GAP = 150;

interface Props {
  points: TimelinePoint[];
  markers: TimelineMarker[];
  start: number;
  end: number;
  label: string;
  /** Snapshots comparés, mis en évidence (A, B). */
  from: number | null;
  to: number | null;
}

export function Chart({ points, markers, start, end, label, from, to }: Props) {
  const t = useT();
  const [ref, size] = useSize<HTMLDivElement>();
  const [hover, setHover] = useState<TimelinePoint | null>(null);
  const width = Math.max(size.width - PAD.left - PAD.right, 1);
  const xOf = (at: number) => PAD.left + ((at - start) / Math.max(end - start, 1)) * width;
  // Repères, de gauche à droite : une rangée de plus quand deux libellés se chevaucheraient.
  const rows: number[] = [];
  const placed = [...markers]
    .sort((m, n) => m.at - n.at)
    .map((m) => {
      const x = xOf(m.at);
      let row = rows.findIndex((r) => x - r >= MARKER_MIN_GAP);
      if (row === -1) row = rows.length;
      rows[row] = x;
      return { m, x, row };
    });
  const top = PAD.top + Math.max(rows.length - 1, 0) * MARKER_ROW;
  const box = { left: PAD.left, top, width, height: Math.max(size.height - top - PAD.bottom, 1) };
  const scale = scaleOf(points, box, start, end);

  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${scale.x(p.at).toFixed(1)},${scale.y(p.used).toFixed(1)}`).join("");
  const bottom = box.top + box.height;
  const first = points[0];
  const last = points[points.length - 1];
  const area = first && last ? `${line}L${scale.x(last.at).toFixed(1)},${bottom}L${scale.x(first.at).toFixed(1)},${bottom}Z` : "";
  const days = [0, 0.25, 0.5, 0.75, 1].map((f) => start + (end - start) * f);

  const focus = (id: number | null) => points.find((p) => p.id === id) ?? null;
  const a = focus(from);
  const b = focus(to);

  return (
    <div
      ref={ref}
      className={styles.chart}
      onPointerMove={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        const x = e.clientX - r.left;
        if (x < box.left || x > box.left + box.width) return setHover(null);
        const at = start + ((x - box.left) / box.width) * (end - start);
        setHover(nearest(points, at));
      }}
      onPointerLeave={() => setHover(null)}
    >
      <svg width={size.width} height={size.height} role="img" aria-label={label} className={styles.svg}>
        {scale.ticks.map((v) => (
          <g key={v}>
            <line x1={box.left} x2={box.left + box.width} y1={scale.y(v)} y2={scale.y(v)} className={styles.grid} />
            <text x={box.left - 10} y={scale.y(v)} className={styles.axisY}>
              {formatBytes(v, t.locale)}
            </text>
          </g>
        ))}
        {days.map((d, i) => (
          <text key={d} x={scale.x(d)} y={bottom + 18} className={styles.axisX} data-edge={i === 0 ? "start" : i === days.length - 1 ? "end" : undefined}>
            {formatDay(d, t.locale)}
          </text>
        ))}
        {placed.map(({ m, x, row }) => (
          <line key={m.actionId} x1={x} x2={x} y1={PAD.top - 6 + row * MARKER_ROW} y2={bottom} className={styles.markerLine} />
        ))}
        {area && <path d={area} className={styles.area} />}
        {line && <path d={line} className={styles.line} />}
        {points.map((p) => (
          <circle key={p.id} cx={scale.x(p.at)} cy={scale.y(p.used)} r={p.id === hover?.id ? 4 : 2.5} className={styles.dot} data-daily={p.source === "daily" || undefined} />
        ))}
        {[a, b].map((p, i) =>
          p ? (
            <g key={i === 0 ? "a" : "b"}>
              <circle cx={scale.x(p.at)} cy={scale.y(p.used)} r={6} className={styles.pick} />
              <text x={scale.x(p.at)} y={scale.y(p.used) - 11} className={styles.pickLabel}>
                {i === 0 ? "A" : "B"}
              </text>
            </g>
          ) : null,
        )}
        {hover && <line x1={scale.x(hover.at)} x2={scale.x(hover.at)} y1={box.top} y2={bottom} className={styles.guide} />}
      </svg>
      {placed.map(({ m, x, row }) => (
        <span
          key={m.actionId}
          className={styles.marker}
          style={{ left: Math.round(Math.min(Math.max(x, box.left), box.left + box.width)), top: PAD.top - 28 + row * MARKER_ROW }}
          title={t("timeline.markerTitle", { date: formatDateTime(m.at, t.locale), title: m.title, size: formatBytes(m.bytes, t.locale) })}
        >
          {t("timeline.marker", { size: formatDelta(-m.bytes, t.locale) })}
        </span>
      ))}
      {hover && (
        <div
          role="tooltip"
          className={`${pop.pop} ${styles.tip}`}
          style={{ left: Math.round(Math.min(scale.x(hover.at) + 12, size.width - 220)), top: Math.round(Math.max(scale.y(hover.used) - 44, 0)) }}
        >
          {t("timeline.point", { date: formatDateTime(hover.at, t.locale), size: formatBytes(hover.used, t.locale), source: t(hover.source === "daily" ? "timeline.source.daily" : "timeline.source.scan") })}
        </div>
      )}
    </div>
  );
}
