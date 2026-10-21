import { formatBytes, formatRelative } from "../../core/format";
import type { Translator } from "../../i18n";
import type { DailyStatus, TimelineData, TimelineSeries } from "../../ipc/types";

/** « C: Système · dossier utilisateur · parcours » ; disque absent : sa dernière lettre. */
export function seriesLabel(s: TimelineSeries, t: Translator): string {
  const drive = s.letter ?? `${s.lastLetter} (${t("timeline.absentDrive")})`;
  const root = s.root === "\\" ? t("timeline.root.volume") : s.profile ? t("timeline.root.profile") : s.root;
  const method = t(s.method === "mft" ? "timeline.method.mft" : "timeline.method.walk");
  return [`${drive} ${s.label}`.trim(), root, method].join(" · ");
}

/** Phrase de projection : jamais de date si la pente est nulle ou négative. */
export function projectionText(d: TimelineData, t: Translator): string {
  const p = d.projection;
  if (p.status === "tooFew") return t("timeline.projection.tooFew", { n: p.points });
  if (p.status === "flat" || p.bytesPerDay === null) return t("timeline.projection.flat");
  const rate = formatBytes(p.bytesPerDay * 7, t.locale);
  if (p.daysToFull === null) return t("timeline.projection.noFree", { rate });
  if (p.daysToFull < 7) return t("timeline.projection.soon", { rate });
  if (p.daysToFull > 730) return t("timeline.projection.far", { rate });
  return t("timeline.projection.growing", { rate, weeks: Math.round(p.daysToFull / 7) });
}

export function dailyText(daily: DailyStatus | null, enabled: boolean, t: Translator, now: number): string[] {
  if (!enabled) return [t("timeline.daily.off")];
  const out = [t("timeline.daily.on")];
  if (daily?.state === "running" && daily.label) out.push(t("timeline.daily.running", { label: daily.label }));
  else if (daily?.lastAt && daily.label && daily.lastOutcome === "done") out.push(t("timeline.daily.last", { when: formatRelative(daily.lastAt, t.locale, now), label: daily.label }));
  if (daily && daily.needsAdmin > 0) out.push(t.plural("timeline.daily.needsAdmin", daily.needsAdmin, { n: daily.needsAdmin }));
  return out;
}
