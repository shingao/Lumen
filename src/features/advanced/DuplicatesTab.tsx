import { cancelDuplicates, checkAllCopies, clearDuplicates, startDuplicates, toggleDuplicate, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { StatusDot } from "../../components/StatusDot";
import { canCheck, keepsOneEach, selectedBytes } from "../../core/duplicates";
import { formatBytes, formatCount, MIB } from "../../core/format";
import { useT } from "../../i18n";
import type { DuplicateGroup } from "../../ipc/types";
import { FileLine, SelectionBar } from "./FileRows";
import styles from "./Advanced.module.css";

function Group({ group }: { group: DuplicateGroup }) {
  const t = useT();
  const checked = useAdvanced((s) => s.checkedDup);
  const any = group.members.some((m) => checked.has(m.id));
  return (
    <section className={styles.group} aria-label={t("dup.group", { count: group.members.length, size: formatBytes(group.size, t.locale) })}>
      <div className={styles.groupHead}>
        <span className={styles.groupTitle}>{t("dup.group", { count: group.members.length, size: formatBytes(group.size, t.locale) })}</span>
        <Button variant="link" onClick={() => checkAllCopies(group.id)}>
          {t("dup.checkGroup")}
        </Button>
        <span className={styles.groupSize}>{t("dup.wasted", { size: formatBytes(group.wasted, t.locale) })}</span>
      </div>
      <ul className={styles.rows}>
        {group.members.map((m) => {
          const on = checked.has(m.id);
          return (
            <FileLine
              key={m.id}
              file={m}
              checked={on}
              disabled={!canCheck(group, checked, m.id)}
              disabledTitle={t("dup.lastCopy")}
              onChange={() => toggleDuplicate(group.id, m.id)}
              badge={any && !on ? <span className={styles.keep}>{t("dup.keep")}</span> : null}
            />
          );
        })}
      </ul>
    </section>
  );
}

/** Doublons : la seule fonction qui lit le contenu des fichiers. Rien n'est coché par défaut. */
export function DuplicatesTab() {
  const t = useT();
  const minMb = useStore((s) => s.settings?.duplicateMinMb ?? 1);
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  const { duplicates, dupProgress, dupCancelling, checkedDup, scanId: loadedFor } = useAdvanced();
  const current = loadedFor === scanId ? duplicates : { status: "idle" as const };
  const result = current.status === "done" ? current.value : null;
  const groups = result?.groups ?? [];
  const n = groups.reduce((a, g) => a + g.members.filter((m) => checkedDup.has(m.id)).length, 0);
  const size = selectedBytes(groups, checkedDup);
  const ratio = dupProgress && dupProgress.total > 0 ? dupProgress.done / dupProgress.total : 0;

  return (
    <>
      <p className={styles.notice}>{t("dup.intro")}</p>
      <div className={styles.toolbar}>
        {current.status === "loading" ? (
          <>
            <div className={styles.progress}>
              <span className={styles.toolLabel}>{dupProgress ? t(`dup.phase.${dupProgress.phase}`) : t("dup.starting")}</span>
              <div className={styles.meter} role="progressbar" aria-valuemin={0} aria-valuemax={dupProgress?.total ?? 0} aria-valuenow={dupProgress?.done ?? 0}>
                <span style={{ width: `${Math.round(ratio * 100)}%` }} />
              </div>
              <span className={styles.mono}>{dupProgress ? `${formatCount(dupProgress.done, t.locale)} / ${formatCount(dupProgress.total, t.locale)}` : ""}</span>
            </div>
            <Button variant="link" onClick={cancelDuplicates} disabled={dupCancelling}>
              {dupCancelling ? t("dup.stopping") : t("dup.stop")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="primary" disabled={scanId === null} onClick={() => void startDuplicates(minMb)}>
              {result ? t("dup.restart") : t("dup.start")}
            </Button>
            <span className={styles.hint}>{t("dup.minSize", { size: formatBytes(minMb * MIB, t.locale) })}</span>
          </>
        )}
      </div>
      {current.status === "failed" && <p className={styles.warning}>{current.message}</p>}
      {result && (
        <>
          <div className={styles.summary}>
            <span className={styles.status}>
              <StatusDot status="review" />
              {t("status.review")}
            </span>
            <span className={styles.summaryText}>{t.plural("dup.summary", groups.length + result.moreGroups, { n: formatCount(groups.length + result.moreGroups, t.locale), size: formatBytes(result.wasted, t.locale) })}</span>
            {groups.length > 0 && (
              <Button variant="link" onClick={() => checkAllCopies()}>
                {t("dup.checkCopies")}
              </Button>
            )}
          </div>
          <p className={styles.hint}>
            {t("dup.stats", {
              read: formatCount(result.stats.read, t.locale),
              cached: formatCount(result.stats.cached, t.locale),
              links: formatCount(result.stats.hardLinks, t.locale),
              cloud: formatCount(result.stats.skippedCloud, t.locale),
            })}
            {" · "}
            {t("dup.recheck")}
          </p>
          {groups.length === 0 ? (
            <p className={styles.empty}>{t("dup.empty")}</p>
          ) : (
            <div className={styles.groups}>
              {groups.map((g) => (
                <Group key={g.id} group={g} />
              ))}
              {result.moreGroups > 0 && <p className={styles.hint}>{t.plural("dup.more", result.moreGroups, { n: formatCount(result.moreGroups, t.locale) })}</p>}
            </div>
          )}
        </>
      )}
      <SelectionBar
        summary={t.plural("dup.selected", n, { n: formatCount(n, t.locale), size: formatBytes(size, t.locale) })}
        onClear={clearDuplicates}
        action={t("adv.clean")}
        disabled={n === 0 || !keepsOneEach(groups, checkedDup)}
        onAction={() => openRequest({ kind: "duplicates", ids: [...checkedDup] })}
      />
    </>
  );
}
