import { useEffect, type ReactNode } from "react";
import { loadRestorePoints, loadSinks, openSystemProtection, setRestorePoint, toggleSystemAction, useAdvanced } from "../../app/advanced";
import { openRequest } from "../../app/cleaning";
import { Button } from "../../components/Button";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { Toggle } from "../../components/Toggle";
import { formatBytes, formatCount, formatDate, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import type { SystemAction, SystemSinks } from "../../ipc/types";
import { Check, SelectionBar } from "./FileRows";
import styles from "./Advanced.module.css";

function Card({ title, size, command, children, control, info }: { title: string; size?: string; command?: string; children: ReactNode; control?: ReactNode; info?: boolean }) {
  const t = useT();
  return (
    <section className={styles.card} aria-label={title}>
      <div className={styles.cardHead}>
        {control}
        <h3 className={styles.cardTitle}>{title}</h3>
        {info && <span className={styles.tag}>{t("sys.info")}</span>}
        {size && <span className={styles.groupSize}>{size}</span>}
      </div>
      <div className={styles.cardBody}>
        {children}
        {!size && !info && <span className={styles.hint}>{t("sys.measured")}</span>}
        {command && <code className={styles.command}>{command}</code>}
      </div>
    </section>
  );
}

/** « Supprimer le plus ancien point de restauration » : jamais sans avoir lu la liste, jamais
 * le dernier point, jamais un point de moins de 7 jours ou de cette session. */
function RestoreCard({ drive }: { drive: string }) {
  const t = useT();
  const read = useAdvanced((s) => s.restorePoints);
  const on = useAdvanced((s) => s.systemActions.includes("oldestRestorePoint"));
  const value = read.status === "done" ? read.value : null;
  const title = t("sys.restore.title");
  const control = (
    <Check
      checked={on}
      disabled={!value?.deletable}
      title={value?.refusal ?? (value ? undefined : t("sys.restore.readFirst"))}
      label={t("sys.include", { title })}
      onChange={(checked) => toggleSystemAction("oldestRestorePoint", checked)}
    />
  );
  return (
    <Card title={title} control={control} command={value?.deletable && value.oldest ? `vssadmin.exe delete shadows /shadow=${value.oldest.id} /quiet` : undefined}>
      <p>{t("sys.restore.text", { drive })}</p>
      {read.status === "idle" && (
        <Button variant="primary" onClick={() => void loadRestorePoints()}>
          {t("sys.restore.read")}
        </Button>
      )}
      {read.status === "loading" && <span className={styles.hint}>{t("sys.restore.reading")}</span>}
      {read.status === "failed" && <span className={styles.warning}>{read.message}</span>}
      {value && (
        <>
          <span className={styles.hint}>{t.plural("sys.restore.count", value.count, { n: formatCount(value.count, t.locale), drive: value.drive })}</span>
          {value.oldest && (
            <span className={value.deletable ? styles.oldest : styles.warning}>
              {t(value.deletable ? "sys.restore.oldest" : "sys.restore.oldestKept", {
                date: formatDate(value.oldest.created, t.locale),
                when: formatRelative(value.oldest.created, t.locale),
                what: value.oldest.description ?? t("sys.restore.noDescription"),
              })}
            </span>
          )}
          {value.refusal && <span className={styles.hint}>{value.refusal}</span>}
        </>
      )}
    </Card>
  );
}

type Hibernate = "keep" | "hibernateReduced" | "hibernateOff";

function Cards({ sinks }: { sinks: SystemSinks }) {
  const t = useT();
  const actions = useAdvanced((s) => s.systemActions);
  const has = (a: SystemAction) => actions.includes(a);
  const include = (a: SystemAction, title: string) => <Check checked={has(a)} label={t("sys.include", { title })} onChange={(on) => toggleSystemAction(a, on)} />;
  const drive = sinks.systemDrive;
  const size = (n: number | null) => (n === null ? undefined : formatBytes(n, t.locale));
  const hibernate: Hibernate = has("hibernateOff") ? "hibernateOff" : has("hibernateReduced") ? "hibernateReduced" : "keep";
  const hibernateOptions: { value: Hibernate; label: string }[] = [
    { value: "keep", label: t("sys.hibernate.keep") },
    { value: "hibernateReduced", label: t("sys.hibernate.reduced") },
    { value: "hibernateOff", label: t("sys.hibernate.off") },
  ];
  return (
    <div className={styles.cards}>
      <Card title={t("sys.components.title")} control={include("components", t("sys.components.title"))} command="Dism.exe /Online /Cleanup-Image /StartComponentCleanup">
        <p>{t("sys.components.text")}</p>
      </Card>
      <Card title={t("sys.hibernate.title")} size={size(sinks.hiberfil) ?? "—"} command={hibernate === "keep" ? undefined : hibernate === "hibernateOff" ? "powercfg.exe /hibernate off" : "powercfg.exe /hibernate /type reduced"}>
        {sinks.hiberfil === null && sinks.hibernateEnabled !== true ? (
          <p>{t("sys.hibernate.disabled")}</p>
        ) : (
          <>
            <p>
              {t("sys.hibernate.text")} {sinks.fastStartup ? t("sys.hibernate.fastStartup") : ""}
            </p>
            <Segmented<Hibernate>
              options={hibernateOptions}
              value={hibernate}
              label={t("sys.hibernate.choice")}
              onChange={(v) => {
                if (v !== "keep") {
                  toggleSystemAction(v, true);
                  return;
                }
                toggleSystemAction("hibernateOff", false);
                toggleSystemAction("hibernateReduced", false);
              }}
            />
          </>
        )}
      </Card>
      <RestoreCard drive={drive} />
      {sinks.windowsOld ? (
        <Card title={t("sys.windowsOld.title")} control={include("windowsOld", t("sys.windowsOld.title"))} command={`cleanmgr.exe /d ${drive}`}>
          <p>{t("sys.windowsOld.text")}</p>
        </Card>
      ) : (
        <Card title={t("sys.windowsOld.title")} size="—" info>
          <p>{t("sys.windowsOld.none", { drive })}</p>
        </Card>
      )}
      <Card title={t("sys.pagefile.title")} size={size(sinks.pagefile) ?? "—"} info>
        <p>{sinks.pagefile === null ? t("sys.pagefile.none", { drive }) : t("sys.pagefile.text")}</p>
        {sinks.swapfile !== null && sinks.swapfile > 0 && <span className={styles.hint}>{t("sys.swapfile", { size: formatBytes(sinks.swapfile, t.locale) })}</span>}
      </Card>
    </div>
  );
}

/** Gouffres système : commandes officielles élevées (une demande UAC chacune), gain mesuré. */
export function SystemTab() {
  const t = useT();
  const { sinks, systemActions, restorePoint } = useAdvanced();

  useEffect(() => {
    if (sinks.status === "idle") void loadSinks();
  }, [sinks.status]);

  const value = sinks.status === "done" ? sinks.value : null;
  const n = systemActions.length;
  const free = value?.free ?? null;
  const summary = [t.plural("sys.selected", n, { n: formatCount(n, t.locale) }), free === null || !value ? null : t("sys.free", { drive: value.systemDrive, free: formatBytes(free, t.locale) })]
    .filter(Boolean)
    .join(" · ");
  return (
    <>
      <p className={styles.notice}>{t("sys.intro")}</p>
      {sinks.status === "loading" && <p className={styles.hint}>{t("adv.loading")}</p>}
      {sinks.status === "failed" && <p className={styles.warning}>{sinks.message}</p>}
      {value && !value.available && <p className={styles.warning}>{t("sys.unavailable")}</p>}
      {value && (
        <>
          <div className={styles.summary}>
            <span className={styles.status}>
              <StatusDot status="review" />
              {t("status.review")}
            </span>
          </div>
          <Cards sinks={value} />
          <div className={styles.restore}>
            <Toggle checked={restorePoint} onChange={setRestorePoint} label={t("sys.restorePoint")} disabled={value.restoreProtection === false} />
            <div className={styles.restoreText}>
              <span className={styles.cardTitle}>{t("sys.restorePoint")}</span>
              {value.restoreProtection === false ? (
                <>
                  <span className={styles.warning}>{t("sys.protection.off", { drive: value.systemDrive })}</span>
                  <span>
                    <Button variant="link" onClick={openSystemProtection}>
                      {t("sys.protection.open")}
                    </Button>
                  </span>
                </>
              ) : (
                <span className={styles.hint}>{t("sys.restorePoint.text", { hours: Math.max(1, Math.round(value.restorePointFrequencyMin / 60)) })}</span>
              )}
              {!restorePoint && <span className={styles.hint}>{t("sys.restorePoint.without")}</span>}
            </div>
          </div>
        </>
      )}
      <SelectionBar
        summary={summary}
        action={t("sys.go")}
        disabled={n === 0 || !value?.available}
        onAction={() => openRequest({ kind: "system", actions: systemActions, restorePoint })}
      />
    </>
  );
}
