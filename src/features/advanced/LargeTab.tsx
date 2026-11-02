import { useEffect } from "react";
import { checkLargest, loadLargest, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatCount } from "../../core/format";
import { useT } from "../../i18n";
import type { FileKind, FileRow } from "../../ipc/types";
import { FileLine, SelectionBar } from "./FileRows";
import { CollapsibleGroup, GroupsToolbar } from "./Groups";
import styles from "./Advanced.module.css";

type Filter = FileKind | "all";

/** Dossier d'un chemin affiché (`~\\Vidéos\\film.mp4` → `~\\Vidéos`). */
function folderOf(path: string): string {
  const i = path.lastIndexOf("\\");
  return i > 0 ? path.slice(0, i) : path;
}

/** Gros fichiers regroupés par dossier, le plus lourd d'abord ; chaque groupe replié par défaut. */
function Folders({ files }: { files: FileRow[] }) {
  const t = useT();
  const checked = useAdvanced((s) => s.checkedLargest);
  const byFolder = new Map<string, FileRow[]>();
  for (const f of files) byFolder.set(folderOf(f.path), [...(byFolder.get(folderOf(f.path)) ?? []), f]);
  const groups = [...byFolder.entries()].map(([path, list]) => ({ path, list, size: list.reduce((a, f) => a + f.size, 0) })).sort((a, b) => b.size - a.size);
  return (
    <div className={styles.groups}>
      <GroupsToolbar tab="largest" keys={groups.map((g) => g.path)} />
      {groups.map((g) => {
        const ids = g.list.map((f) => f.id);
        const n = ids.filter((id) => checked.has(id)).length;
        return (
          <CollapsibleGroup
            key={g.path}
            tab="largest"
            id={g.path}
            title={g.path}
            count={g.list.length}
            size={g.size}
            latest={Math.max(0, ...g.list.map((f) => f.modified))}
            check={{ checked: n === ids.length && n > 0, partial: n > 0, label: t("adv.checkGroup", { name: g.path }), onChange: (on) => checkLargest(ids, on) }}
          >
            {g.list.map((f) => (
              <FileLine key={f.id} file={f} checked={checked.has(f.id)} onChange={(on) => checkLargest([f.id], on)} />
            ))}
          </CollapsibleGroup>
        );
      })}
    </div>
  );
}

/** Les 100 plus gros fichiers de l'analyse, filtrables par type : toujours « À vérifier ». */
export function LargeTab() {
  const t = useT();
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  const { largest, largestKind, checkedLargest, scanId: loadedFor } = useAdvanced();

  useEffect(() => {
    if (scanId !== null && (loadedFor !== scanId || largest.status === "idle")) void loadLargest(largestKind);
  }, [scanId, loadedFor, largest.status, largestKind]);

  const result = largest.status === "done" ? largest.value : null;
  const files = result?.files ?? [];
  const selected = files.filter((f) => checkedLargest.has(f.id));
  const size = selected.reduce((a, f) => a + f.size, 0);
  const total = files.reduce((a, f) => a + f.size, 0);
  const options: { value: Filter; label: string }[] = [
    { value: "all", label: t("large.all") },
    ...(result?.kinds ?? []).map(([k, n]) => ({ value: k as Filter, label: `${t(`large.kind.${k}`)} · ${formatCount(n, t.locale)}` })),
  ];

  return (
    <>
      <div className={styles.toolbar}>
        <span className={styles.toolLabel}>{t("large.filter")}</span>
        <Segmented<Filter> options={options} value={largestKind ?? "all"} onChange={(k) => void loadLargest(k === "all" ? null : k)} label={t("large.filter")} />
      </div>
      {largest.status === "loading" && <p className={styles.hint}>{t("adv.loading")}</p>}
      {largest.status === "failed" && <p className={styles.warning}>{largest.message}</p>}
      {result && (
        <>
          <div className={styles.summary}>
            <span className={styles.status}>
              <StatusDot status="review" />
              {t("status.review")}
            </span>
            <span className={styles.summaryText}>{t.plural("large.summary", files.length, { n: formatCount(files.length, t.locale), size: formatBytes(total, t.locale) })}</span>
          </div>
          <p className={styles.hint}>
            {t("adv.note")} {t("large.note")}
          </p>
          {files.length === 0 ? <p className={styles.empty}>{t("adv.empty")}</p> : <Folders files={files} />}
        </>
      )}
      <SelectionBar
        summary={t.plural("adv.selected", selected.length, { n: formatCount(selected.length, t.locale), size: formatBytes(size, t.locale) })}
        onClear={() => checkLargest([...checkedLargest], false)}
        action={t("adv.clean")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "largest", ids: selected.map((f) => f.id) })}
      />
    </>
  );
}
