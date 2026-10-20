import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cancelCleanup, closeCleanup, confirmCleanup, confirmPermanent, permanentPhraseOk, setCleanMode } from "../../app/cleaning";
import { startScan } from "../../app/scanning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import pop from "../../components/Popover.module.css";
import { Segmented } from "../../components/Segmented";
import { formatBytes, formatCount } from "../../core/format";
import { useT, type Translator } from "../../i18n";
import type { CleanMode, CleanPlan, CleanReport } from "../../ipc/types";
import styles from "./CleanDialog.module.css";

/** Méthode affichée : Corbeille, définitif, commande officielle, ou les deux. */
function methodOf(plan: CleanPlan, t: Translator): string {
  const actions = plan.commands.some((c) => !c.prerequisite);
  const items = plan.items.length + plan.moreItems > 0;
  const del = t(plan.mode === "trash" ? "clean.method.trash" : "clean.method.permanent");
  if (items && actions) return t("clean.method.both", { method: del });
  return items ? del : t("clean.method.command");
}

/** « Mettre 44,2 Go à la Corbeille », « Supprimer définitivement 44,2 Go », « Lancer les commandes · 13,4 Go ». */
function cleanButtonLabel(plan: CleanPlan, t: Translator): string {
  const size = formatBytes(plan.total, t.locale);
  if (plan.items.length + plan.moreItems === 0) return t("clean.go.command", { size });
  return t(plan.mode === "trash" ? "clean.go.trash" : "clean.go.permanent", { size });
}

function Summary({ plan }: { plan: CleanPlan }) {
  const t = useT();
  const disks = plan.volumes
    .map((v) => (v.free === null ? v.volume : t("clean.disk.free", { disk: v.volume.replace(/\\$/, ""), free: formatBytes(v.free, t.locale) })))
    .join(" · ");
  return (
    <dl className={styles.summary}>
      <div className={styles.cell}>
        <dt>{t("clean.total")}</dt>
        <dd className={styles.big}>{formatBytes(plan.total, t.locale)}</dd>
      </div>
      <div className={styles.cell}>
        <dt>{t("clean.count")}</dt>
        <dd className={styles.num}>{formatCount(plan.count, t.locale)}</dd>
      </div>
      <div className={styles.cell}>
        <dt>{t("clean.method")}</dt>
        <dd>{methodOf(plan, t)}</dd>
      </div>
      <div className={styles.cell}>
        <dt>{t("clean.disk")}</dt>
        <dd className={styles.num}>{disks || "—"}</dd>
      </div>
    </dl>
  );
}

function PlanList({ plan }: { plan: CleanPlan }) {
  const t = useT();
  const n = plan.items.length + plan.moreItems + plan.commands.length;
  return (
    <details className={styles.list}>
      <summary>{t.plural("clean.list", n, { n: formatCount(n, t.locale) })}</summary>
      <ul>
        {plan.commands.map((c) => (
          <li key={c.line} className={styles.entry}>
            <span className={styles.entryText}>
              <code className={styles.mono}>{c.line}</code>
              <span className={styles.hint}>
                {c.prerequisite ? t("clean.prerequisite") : c.explanation}
                {c.needsElevation ? ` · ${t("clean.needsAdmin")}` : ""}
              </span>
            </span>
            {c.size > 0 && <span className={styles.size}>{formatBytes(c.size, t.locale)}</span>}
          </li>
        ))}
        {plan.items.map((i) => (
          <li key={i.path} className={styles.entry}>
            <span className={styles.entryText}>
              <code className={styles.mono}>{i.directory ? `${i.path}\\` : i.path}</code>
              <span className={styles.hint}>{i.ruleName}</span>
            </span>
            <span className={styles.size}>{formatBytes(i.size, t.locale)}</span>
          </li>
        ))}
        {plan.moreItems > 0 && <li className={styles.more}>{t.plural("clean.more", plan.moreItems, { n: formatCount(plan.moreItems, t.locale) })}</li>}
      </ul>
    </details>
  );
}

function Refused({ plan }: { plan: CleanPlan }) {
  const t = useT();
  const n = plan.refused.length + plan.skippedLocked;
  if (n === 0) return null;
  return (
    <details className={`${styles.list} ${styles.warn}`}>
      <summary>{t.plural("clean.refused", n, { n: formatCount(n, t.locale) })}</summary>
      <ul>
        {plan.refused.map((r) => (
          <li key={r.path} className={styles.entry}>
            <span className={styles.entryText}>
              <code className={styles.mono}>{r.path}</code>
              <span className={styles.hint}>{r.reason}</span>
            </span>
          </li>
        ))}
        {plan.skippedLocked > 0 && <li className={styles.more}>{t.plural("clean.locked", plan.skippedLocked, { n: plan.skippedLocked })}</li>}
      </ul>
    </details>
  );
}

function Confirm({ plan }: { plan: CleanPlan }) {
  const t = useT();
  const allowPermanent = useStore((s) => s.settings?.permanentDelete ?? false);
  const deletes = plan.items.length + plan.moreItems > 0;
  const modes: { value: CleanMode; label: string }[] = [
    { value: "trash", label: t("clean.mode.trash") },
    { value: "permanent", label: t("clean.mode.permanent") },
  ];
  return (
    <>
      <Summary plan={plan} />
      {allowPermanent && deletes && (
        <div className={styles.mode}>
          <Segmented<CleanMode> options={modes} value={plan.mode} onChange={setCleanMode} label={t("clean.mode")} />
        </div>
      )}
      <PlanList plan={plan} />
      <Refused plan={plan} />
      <p className={styles.note}>{t(plan.mode === "trash" ? "clean.note.trash" : "clean.note.permanent")}</p>
      <div className={styles.actions}>
        <Button variant="link" onClick={cancelCleanup}>
          {t("clean.cancel")}
        </Button>
        <Button variant="clean" disabled={plan.count === 0} onClick={confirmCleanup}>
          {cleanButtonLabel(plan, t)}
        </Button>
      </div>
    </>
  );
}

function Permanent({ plan }: { plan: CleanPlan }) {
  const t = useT();
  const [typed, setTyped] = useState("");
  const total = formatBytes(plan.total, t.locale);
  const ok = permanentPhraseOk(typed, total);
  return (
    <form
      className={styles.permanent}
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) confirmPermanent();
      }}
    >
      <p className={styles.warnText}>{t("clean.permanent.text", { size: total, n: formatCount(plan.count, t.locale) })}</p>
      <label className={styles.field}>
        <span>{t("clean.permanent.label", { size: total })}</span>
        <input autoFocus className={styles.input} value={typed} onChange={(e) => setTyped(e.target.value)} spellCheck={false} autoComplete="off" />
      </label>
      <div className={styles.actions}>
        <Button variant="link" onClick={cancelCleanup}>
          {t("clean.back")}
        </Button>
        <Button variant="clean" type="submit" disabled={!ok} className={styles.danger}>
          {t("clean.go.permanent", { size: total })}
        </Button>
      </div>
    </form>
  );
}

function Running() {
  const t = useT();
  const progress = useStore((s) => s.clean.progress);
  const cancelling = useStore((s) => s.clean.cancelling);
  const ratio = progress && progress.total > 0 ? Math.min(1, progress.index / progress.total) : 0;
  return (
    <>
      <div className={styles.bar} role="progressbar" aria-valuemin={0} aria-valuemax={progress?.total ?? 0} aria-valuenow={progress?.index ?? 0}>
        <span style={{ width: `${Math.round(ratio * 100)}%` }} />
      </div>
      <p className={styles.current}>
        <span className={styles.hint}>{t(progress?.phase === "command" ? "clean.running.command" : "clean.running.item")}</span>
        <code className={styles.mono}>{progress?.label ?? "…"}</code>
      </p>
      <p className={styles.hint}>
        {progress && progress.verified > 0 ? `${t.plural("clean.running.verified", progress.verified, { n: formatCount(progress.verified, t.locale) })} · ` : ""}
        {t("clean.running.freed", { size: formatBytes(progress?.freed ?? 0, t.locale) })}
      </p>
      <div className={styles.actions}>
        <Button variant="link" onClick={cancelCleanup} disabled={cancelling}>
          {cancelling ? t("clean.cancelling") : t("clean.stop")}
        </Button>
      </div>
    </>
  );
}

function ReportView({ report }: { report: CleanReport }) {
  const t = useT();
  const go = useStore((s) => s.go);
  const target = useStore((s) => s.target);
  const notDone = report.items.filter((i) => i.outcome !== "done" || i.issues.length > 0);
  const counts: [string, number][] = [
    [t("clean.report.done"), report.done + report.partial],
    [t("clean.report.skipped"), report.skipped],
    [t("clean.report.refused"), report.refused],
    [t("clean.report.failed"), report.failed],
    [t("clean.report.cancelled"), report.cancelled],
  ];
  return (
    <>
      <dl className={styles.summary}>
        <div className={styles.cell}>
          <dt>{t("clean.report.freed")}</dt>
          <dd className={styles.big}>{formatBytes(report.freed, t.locale)}</dd>
        </div>
        <div className={styles.cell}>
          <dt>{t("clean.report.measured")}</dt>
          <dd className={styles.num}>{report.measured === null ? "—" : formatBytes(Math.max(0, report.measured), t.locale)}</dd>
        </div>
        {counts
          .filter(([, n], i) => i === 0 || n > 0)
          .map(([label, n]) => (
            <div key={label} className={styles.cell}>
              <dt>{label}</dt>
              <dd className={styles.num}>{formatCount(n, t.locale)}</dd>
            </div>
          ))}
      </dl>
      {report.commands.length > 0 && (
        <ul className={styles.commands}>
          {report.commands.map((c) => (
            <li key={c.line} className={styles.entry}>
              <span className={styles.entryText}>
                <code className={styles.mono}>{c.line}</code>
                {c.detail && <span className={styles.hint}>{c.detail}</span>}
              </span>
              <span className={styles.status} data-status={c.status}>
                {t(`clean.command.${c.status}`)}
              </span>
            </li>
          ))}
        </ul>
      )}
      {notDone.length > 0 && (
        <details className={styles.list}>
          <summary>{t.plural("clean.report.left", notDone.length, { n: formatCount(notDone.length, t.locale) })}</summary>
          <ul>
            {notDone.map((i) => (
              <li key={i.path} className={styles.entryBlock}>
                <span className={styles.entry}>
                  <code className={styles.mono}>{i.path}</code>
                  <span className={styles.status} data-status={i.outcome}>
                    {t(`clean.outcome.${i.outcome}`)}
                  </span>
                </span>
                {i.reason && <span className={styles.hint}>{i.reason}</span>}
                {i.issues.map((x) => (
                  <span key={x.path} className={styles.issue}>
                    <code className={styles.mono}>{x.path}</code> — {x.reason}
                  </span>
                ))}
                {i.moreIssues > 0 && <span className={styles.hint}>{t.plural("clean.more", i.moreIssues, { n: i.moreIssues })}</span>}
              </li>
            ))}
          </ul>
        </details>
      )}
      {report.journalError && <p className={styles.warnText}>{report.journalError}</p>}
      <div className={styles.actions}>
        <Button
          variant="link"
          onClick={() => {
            closeCleanup();
            go("journal");
          }}
        >
          {t("clean.report.journal")}
        </Button>
        <Button
          variant="primary"
          onClick={() => {
            closeCleanup();
            void startScan(target);
          }}
        >
          {t("clean.report.rescan")}
        </Button>
      </div>
    </>
  );
}

function titleOf(step: string, report: CleanReport | null, plan: CleanPlan | null, t: Translator): string {
  if (step === "running") return t("clean.running.title");
  if (step === "permanent") return t("clean.permanent.title");
  if (step === "error") return t("clean.error.title");
  if (step === "report" && report) return t(`clean.report.title.${report.status}`);
  return plan?.mode === "permanent" ? t("clean.title.permanent") : t("clean.title");
}

/** Confirmation, avancement et bilan d'un nettoyage, dans une fenêtre modale. */
export function CleanDialog() {
  const t = useT();
  const clean = useStore((s) => s.clean);
  const ref = useRef<HTMLDivElement>(null);
  const open = clean.step !== "closed";

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        cancelCleanup();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const first = ref.current?.querySelector<HTMLElement>("input, button:not(:disabled), summary");
    first?.focus();
  }, [open, clean.step]);

  if (!open) return null;
  const title = titleOf(clean.step, clean.report, clean.plan, t);
  return createPortal(
    <div className={styles.scrim}>
      <div ref={ref} role="dialog" aria-modal="true" aria-label={title} className={`${pop.pop} ${styles.dialog}`}>
        <h2 className={styles.title}>{title}</h2>
        {clean.step === "loading" && <p className={styles.hint}>{t("clean.loading")}</p>}
        {clean.step === "confirm" && clean.plan && <Confirm plan={clean.plan} />}
        {clean.step === "permanent" && clean.plan && <Permanent plan={clean.plan} />}
        {clean.step === "running" && <Running />}
        {clean.step === "report" && clean.report && <ReportView report={clean.report} />}
        {clean.step === "error" && (
          <>
            <p className={styles.warnText}>{clean.message}</p>
            <div className={styles.actions}>
              <Button variant="primary" onClick={closeCleanup}>
                {t("clean.close")}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>,
    document.body,
  );
}
