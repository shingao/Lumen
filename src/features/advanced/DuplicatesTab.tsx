import { cancelDuplicates, checkAllCopies, clearDuplicates, startDuplicates, toggleDuplicate, uncheckGroup, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { StatusDot } from "../../components/StatusDot";
import { canCheck, keepsOneEach, selectedBytes } from "../../core/duplicates";
import { formatBytes, formatCount, MIB } from "../../core/format";
import { useT } from "../../i18n";
import type { DuplicateGroup } from "../../ipc/types";
import { FileLine, SelectionBar } from "./FileRows";
import { CollapsibleGroup, GroupsToolbar } from "./Groups";
import styles from "./Advanced.module.css";

function Group({ group }: { group: DuplicateGroup }) {
  const t = useT();
  const checked = useAdvanced((s) => s.checkedDup);
  const any = group.members.some((m) => checked.has(m.id));
  const n = group.members.filter((m) => checked.has(m.id)).length;
  const name = group.members[0]?.name ?? "";
  return (
    <CollapsibleGroup
      tab="duplicates"
      id={String(group.id)}
      title={name}
      meta={t("dup.group", { count: group.members.length, size: formatBytes(group.size, t.locale) })}
      count={group.members.length}
      size={group.size * group.members.length}
      latest={Math.max(0, ...group.members.map((m) => m.modified))}
      // Case du groupe : toutes les copies sauf l'exemplaire gardé (jamais tous les exemplaires).
      check={{
        checked: n >= group.members.length - 1 && n > 0,
        partial: n > 0,
        label: t("dup.checkGroupOf", { name }),
        onChange: (on) => (on ? checkAllCopies(group.id) : uncheckGroup(group.id)),
      }}
      extra={<span className={styles.groupMeta}>{t("dup.wasted", { size: formatBytes(group.wasted, t.locale) })}</span>}
    >
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
    </CollapsibleGroup>
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
              <GroupsToolbar tab="duplicates" keys={groups.map((g) => String(g.id))} />
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
