import { useEffect, useState } from "react";
import { useCopyFlash } from "../../app/clipboard";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { formatBytes, formatCount, formatDate } from "../../core/format";
import { currentT, useT } from "../../i18n";
import { backend } from "../../ipc/client";
import type { JournalAction, JournalDetail, JournalItem } from "../../ipc/types";
import { MovesList } from "../move/Moves";
import screens from "../screens/Screens.module.css";
import styles from "./JournalScreen.module.css";

function ItemRow({ item, onRestored }: { item: JournalItem; onRestored(): void }) {
  const t = useT();
  const showToast = useStore((s) => s.showToast);
  const [busy, setBusy] = useState(false);
  return (
    <li className={styles.item}>
      <span className={styles.itemText}>
        <code className={styles.mono}>{item.directory ? `${item.path}\\` : item.path}</code>
        {item.reason && <span className={styles.hint}>{item.reason}</span>}
      </span>
      <span className={styles.size}>{item.size > 0 ? formatBytes(item.size, t.locale) : ""}</span>
      <span className={styles.status} data-status={item.restoredAt ? "restored" : item.result}>
        {item.restoredAt ? t("journal.restored") : t(`journal.result.${item.result}`)}
      </span>
      {item.restorable && (
        <Button
          variant="link"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            backend
              .restore(item.id)
              .then(() => {
                showToast(currentT()("journal.restoredToast"));
                onRestored();
              })
              .catch((err: unknown) => showToast(String(err)))
              .finally(() => setBusy(false));
          }}
        >
          {t("journal.restore")}
        </Button>
      )}
    </li>
  );
}

function Detail({ action }: { action: JournalAction }) {
  const t = useT();
  const [detail, setDetail] = useState<JournalDetail | null>(null);
  const [version, setVersion] = useState(0);
  const [copied, copy] = useCopyFlash();
  const showToast = useStore((s) => s.showToast);

  useEffect(() => {
    let alive = true;
    backend
      .journalDetail(action.id)
      .then((d) => alive && setDetail(d))
      .catch((err: unknown) => console.error("journal :", err));
    return () => {
      alive = false;
    };
  }, [action.id, version]);

  if (!detail) return <p className={styles.hint}>{t("journal.loading")}</p>;
  return (
    <div className={styles.detail}>
      {detail.commands.length > 0 && (
        <ul className={styles.items}>
          {detail.commands.map((c, i) => (
            <li key={i} className={styles.command}>
              <details>
                <summary className={styles.item}>
                  <span className={styles.itemText}>
                    <code className={styles.mono}>{[c.program ?? "—", ...c.args].join(" ")}</code>
                    <span className={styles.hint}>{t(c.role === "prerequisite" ? "journal.prerequisite" : "journal.command")}</span>
                  </span>
                  <span className={styles.status} data-status={c.status}>
                    {t(`clean.command.${c.status}`)}
                  </span>
                </summary>
                {(c.stdout || c.stderr) && <pre className={styles.output}>{[c.stdout, c.stderr].filter(Boolean).join("\n")}</pre>}
                {c.logPath && (
                  <>
                    <p className={styles.hint}>{t("journal.log", { path: c.logPath })}</p>
                    {c.logTail ? <pre className={styles.output}>{c.logTail}</pre> : <p className={styles.hint}>{t("journal.logMissing")}</p>}
                  </>
                )}
              </details>
            </li>
          ))}
        </ul>
      )}
      <ul className={styles.items}>
        {detail.items.map((item) => (
          <ItemRow key={item.id} item={item} onRestored={() => setVersion((v) => v + 1)} />
        ))}
      </ul>
      <div className={styles.detailActions}>
        <Button
          variant="primary"
          onClick={() => void copy(backend.journalScript(action.id)).then((ok) => ok && showToast(currentT()("journal.scriptCopied")))}
        >
          {copied ? t("reco.copied") : t("journal.copyScript")}
        </Button>
      </div>
    </div>
  );
}

function ActionRow({ action, open, onToggle }: { action: JournalAction; open: boolean; onToggle(): void }) {
  const t = useT();
  const left = action.itemsSkipped + action.itemsRefused + action.itemsFailed + action.itemsCancelled;
  return (
    <li className={styles.action}>
      <button type="button" className={styles.head} aria-expanded={open} onClick={onToggle}>
        <span className={styles.date}>{formatDate(action.startedAt, t.locale)}</span>
        <span className={styles.titleText}>
          <span className={styles.name}>{action.title}</span>
          <span className={styles.hint}>
            {t(`journal.kind.${action.kind}`)}
            {" · "}
            {["system", "docker", "wsl"].includes(action.kind) ? t("clean.method.command") : t(action.method === "trash" ? "clean.method.trash" : "clean.method.permanent")}
            {" · "}
            {t.plural("journal.done", action.itemsDone + action.itemsPartial, { n: formatCount(action.itemsDone + action.itemsPartial, t.locale) })}
            {left > 0 ? ` · ${t.plural("journal.left", left, { n: formatCount(left, t.locale) })}` : ""}
          </span>
        </span>
        <span className={styles.status} data-status={action.status}>
          {t(`journal.status.${action.status}`)}
        </span>
        <span className={styles.freed}>
          {formatBytes(action.bytesFreed, t.locale)}
          {action.bytesMeasured !== null && (
            <span className={styles.hint}>{t("journal.measured", { size: formatBytes(Math.max(0, action.bytesMeasured), t.locale) })}</span>
          )}
        </span>
      </button>
      {open && action.aborted && <p className={styles.aborted}>{t("journal.aborted", { reason: action.aborted })}</p>}
      {open && action.withoutRestorePoint && <p className={styles.aborted}>{t("journal.withoutRestorePoint")}</p>}
      {open && <Detail action={action} />}
    </li>
  );
}

/** Journal : chaque nettoyage, ses éléments, ses commandes, et « Restaurer depuis la Corbeille ». */
export function JournalScreen() {
  const t = useT();
  const version = useStore((s) => s.journalVersion);
  const [actions, setActions] = useState<JournalAction[] | null>(null);
  const [open, setOpen] = useState<number | null>(null);

  useEffect(() => {
    let alive = true;
    backend
      .journal()
      .then((a) => alive && setActions(a))
      .catch((err: unknown) => {
        console.error("journal :", err);
        if (alive) setActions([]);
      });
    return () => {
      alive = false;
    };
  }, [version]);

  return (
    <section aria-label={t("screen.journal.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.journal.title")}</h2>
          <p className={screens.subtitle}>{t("screen.journal.subtitle")}</p>
        </div>
      </div>
      <MovesList />
      {actions !== null && actions.length === 0 ? (
        <div className={screens.empty}>
          <p>{t("screen.journal.empty")}</p>
        </div>
      ) : (
        <ul className={styles.actions}>
          {(actions ?? []).map((a) => (
            <ActionRow key={a.id} action={a} open={open === a.id} onToggle={() => setOpen(open === a.id ? null : a.id)} />
          ))}
        </ul>
      )}
    </section>
  );
}
