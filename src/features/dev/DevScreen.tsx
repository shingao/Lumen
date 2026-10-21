import { useEffect, useState } from "react";
import { checkArtifacts, checkCaches, checkWsl, ensureScanData, loadDocker, loadWsl, useDevGames } from "../../app/devgames";
import { openRequest } from "../../app/cleaning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { Segmented } from "../../components/Segmented";
import { StatusDot } from "../../components/StatusDot";
import { sortProjects, type ProjectSort } from "../../core/devgames";
import { formatBytes, formatCount, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import type { DevProject } from "../../ipc/types";
import adv from "../advanced/Advanced.module.css";
import { Check, SelectionBar } from "../advanced/FileRows";
import screens from "../screens/Screens.module.css";
import styles from "./Dev.module.css";

type DevTab = "projects" | "caches" | "docker" | "wsl";
function ProjectRow({ project }: { project: DevProject }) {
  const t = useT();
  const checked = useDevGames((s) => s.checkedArtifacts);
  const ids = project.artifacts.map((a) => a.id);
  const n = ids.filter((id) => checked.has(id)).length;
  const none = ids.length === 0;
  return (
    <li className={adv.row} data-checked={n > 0 || undefined} aria-label={project.name}>
      <div className={styles.project}>
        <Check
          checked={n === ids.length && n > 0}
          partial={n > 0}
          disabled={none}
          title={none ? t(project.lockfile ? "dev.noArtifacts" : "dev.noLockfile") : undefined}
          label={t("dev.checkProject", { name: project.name })}
          onChange={(on) => checkArtifacts(ids, on)}
        />
        <span className={adv.rowText}>
          <span className={adv.rowName}>
            {project.name}
            {project.language && <span className={adv.tag}>{project.language}</span>}
          </span>
          <span className={adv.rowPath}>{project.path}</span>
        </span>
        <span className={styles.col}>
          {project.lastActivity === null ? "—" : t(project.activitySource === "git" ? "dev.activity.git" : "dev.activity.files", { when: formatRelative(project.lastActivity, t.locale) })}
        </span>
        <span className={styles.num} title={project.git ? t("dev.gitSize", { size: formatBytes(project.gitSize, t.locale) }) : undefined}>
          {formatBytes(project.sourceSize, t.locale)}
        </span>
        <span className={adv.rowSize}>{none ? "—" : formatBytes(project.artifactsSize, t.locale)}</span>
        <Button variant="link" disabled={none} onClick={() => openRequest({ kind: "artifacts", ids })}>
          {t("dev.purge")}
        </Button>
      </div>
      {none ? (
        <p className={styles.sub}>{t(project.lockfile ? "dev.noArtifacts" : "dev.noLockfile")}</p>
      ) : (
        <ul className={styles.artifacts} aria-label={t("dev.artifactsOf", { name: project.name })}>
          {project.artifacts.map((a) => (
            <li key={a.id} className={styles.artifact}>
              <label className={styles.artifactLabel}>
                <Check checked={checked.has(a.id)} label={t("adv.check", { name: a.path })} onChange={(on) => checkArtifacts([a.id], on)} />
                <StatusDot status={a.status} />
                <code>{a.path}</code>
                <span className={styles.artifactSize}>{formatBytes(a.size, t.locale)}</span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function ProjectsTab() {
  const t = useT();
  const dev = useDevGames((s) => s.dev);
  const checked = useDevGames((s) => s.checkedArtifacts);
  const [sort, setSort] = useState<ProjectSort>("inactive");
  const value = dev.status === "done" ? dev.value : null;
  const projects = value ? sortProjects(value.projects, sort) : [];
  const artifacts = projects.flatMap((p) => p.artifacts);
  const selected = artifacts.filter((a) => checked.has(a.id));
  const total = artifacts.reduce((a, x) => a + x.size, 0);
  const size = selected.reduce((a, x) => a + x.size, 0);
  const sorts: { value: ProjectSort; label: string }[] = [
    { value: "inactive", label: t("dev.sort.inactive") },
    { value: "artifacts", label: t("dev.sort.artifacts") },
  ];
  return (
    <>
      <p className={adv.notice}>{t("dev.note")}</p>
      {dev.status === "loading" && <p className={adv.hint}>{t("adv.loading")}</p>}
      {dev.status === "failed" && <p className={adv.warning}>{dev.message}</p>}
      {value && (
        <>
          <div className={adv.toolbar}>
            <span className={adv.summaryText}>{t.plural("dev.summary", projects.length, { n: formatCount(projects.length, t.locale), size: formatBytes(total, t.locale) })}</span>
            <span className={styles.spacer} />
            <span className={adv.toolLabel}>{t("dev.sort")}</span>
            <Segmented<ProjectSort> options={sorts} value={sort} onChange={setSort} label={t("dev.sort")} />
          </div>
          {projects.length === 0 ? (
            <p className={adv.empty}>{t("dev.none")}</p>
          ) : (
            <section className={adv.list} aria-label={t("dev.projects")}>
              <div className={styles.head} aria-hidden="true">
                <span />
                <span>{t("dev.col.project")}</span>
                <span>{t("dev.col.activity")}</span>
                <span className={styles.right}>{t("dev.col.source")}</span>
                <span className={styles.right}>{t("dev.col.artifacts")}</span>
                <span />
              </div>
              <ul className={adv.rows}>
                {projects.map((p) => (
                  <ProjectRow key={p.id} project={p} />
                ))}
              </ul>
            </section>
          )}
        </>
      )}
      <SelectionBar
        summary={t.plural("dev.selected", selected.length, { n: formatCount(selected.length, t.locale), size: formatBytes(size, t.locale) })}
        onClear={() => checkArtifacts([...checked], false)}
        action={t("dev.purgeSelection")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "artifacts", ids: selected.map((a) => a.id) })}
      />
    </>
  );
}

function CachesTab() {
  const t = useT();
  const dev = useDevGames((s) => s.dev);
  const checked = useDevGames((s) => s.checkedCaches);
  const caches = dev.status === "done" ? dev.value.caches : [];
  const selected = caches.filter((c) => checked.has(c.id));
  const size = selected.reduce((a, c) => a + c.size, 0);
  return (
    <>
      <p className={adv.notice}>{t("dev.caches.note")}</p>
      {dev.status === "loading" && <p className={adv.hint}>{t("adv.loading")}</p>}
      {dev.status === "done" &&
        (caches.length === 0 ? (
          <p className={adv.empty}>{t("dev.caches.none")}</p>
        ) : (
          <ul className={`${adv.list} ${adv.rows}`} aria-label={t("dev.tab.caches")}>
            {caches.map((c) => (
              <li key={c.id} className={adv.row} data-checked={checked.has(c.id) || undefined}>
                <label className={adv.rowLabel}>
                  <Check checked={checked.has(c.id)} label={t("adv.check", { name: c.name })} onChange={(on) => checkCaches([c.id], on)} />
                  <span className={adv.rowText}>
                    <span className={adv.rowName}>{c.name}</span>
                    <span className={adv.rowPath}>{c.path}</span>
                  </span>
                  <span className={adv.rowMeta}>
                    <span className={`${adv.status} ${c.status === "safe" ? styles.safe : ""}`}>
                      <StatusDot status={c.status} />
                      {t(c.status === "safe" ? "status.safe" : "status.review")}
                    </span>
                    {c.command && <span className={adv.tag}>{t("dev.caches.command")}</span>}
                  </span>
                  <span className={adv.rowSize}>{formatBytes(c.size, t.locale)}</span>
                </label>
              </li>
            ))}
          </ul>
        ))}
      <SelectionBar
        summary={t.plural("dev.selected", selected.length, { n: formatCount(selected.length, t.locale), size: formatBytes(size, t.locale) })}
        onClear={() => checkCaches([...checked], false)}
        action={t("adv.clean")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "recos", ids: selected.map((c) => c.id) })}
      />
    </>
  );
}

function DockerTab() {
  const t = useT();
  const docker = useDevGames((s) => s.docker);
  useEffect(() => {
    if (docker.status === "idle") void loadDocker();
  }, [docker.status]);
  const value = docker.status === "done" ? docker.value : null;
  return (
    <>
      <p className={adv.notice}>{t("docker.note")}</p>
      {docker.status === "loading" && <p className={adv.hint}>{t("docker.loading")}</p>}
      {docker.status === "failed" && <p className={adv.warning}>{docker.message}</p>}
      {value && !value.running && (
        <div className={adv.empty}>
          <p>{t(value.reason === "notInstalled" ? "docker.notInstalled" : "docker.notRunning")}</p>
          <Button variant="primary" onClick={() => void loadDocker()}>
            {t("docker.retry")}
          </Button>
        </div>
      )}
      {value?.running &&
        (value.images.length === 0 ? (
          <p className={adv.empty}>{t("docker.none")}</p>
        ) : (
          <section className={adv.group} aria-label={t("docker.title")}>
            <div className={adv.groupHead}>
              <span className={adv.groupTitle}>{t.plural("docker.images", value.images.length, { n: formatCount(value.images.length, t.locale) })}</span>
              <span className={adv.groupSize}>{formatBytes(value.total, t.locale)}</span>
            </div>
            <ul className={adv.rows}>
              {value.images.map((i) => (
                <li key={i.id} className={adv.row}>
                  <div className={styles.line}>
                    <code className={adv.mono}>{i.id}</code>
                    <span className={adv.rowPath}>
                      {i.repository}:{i.tag}
                    </span>
                    <span className={adv.rowMeta}>{i.created}</span>
                    <span className={adv.rowSize}>{formatBytes(i.size, t.locale)}</span>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        ))}
      <SelectionBar
        summary={value?.running ? t.plural("docker.images", value.images.length, { n: formatCount(value.images.length, t.locale) }) : t("docker.notRunning.short")}
        action={t("docker.prune")}
        disabled={!value?.running || value.images.length === 0}
        onAction={() => openRequest({ kind: "dockerPrune" })}
      />
    </>
  );
}

function WslTab() {
  const t = useT();
  const wsl = useDevGames((s) => s.wsl);
  const checked = useDevGames((s) => s.checkedWsl);
  useEffect(() => {
    if (wsl.status === "idle") void loadWsl();
  }, [wsl.status]);
  const value = wsl.status === "done" ? wsl.value : null;
  const disks = value?.disks ?? [];
  const selected = disks.filter((d) => checked.has(d.id));
  return (
    <>
      <p className={adv.warning} role="note">
        {t("wsl.shutdown")}
      </p>
      <p className={adv.notice}>
        {t(value?.hyperv ? "wsl.method.hyperv" : "wsl.method.diskpart")} {t("wsl.never")}
      </p>
      {wsl.status === "loading" && <p className={adv.hint}>{t("adv.loading")}</p>}
      {wsl.status === "failed" && <p className={adv.warning}>{wsl.message}</p>}
      {value &&
        (disks.length === 0 ? (
          <p className={adv.empty}>{t("wsl.none")}</p>
        ) : (
          <ul className={`${adv.list} ${adv.rows}`} aria-label={t("dev.tab.wsl")}>
            {disks.map((d) => (
              <li key={d.id} className={adv.row} data-checked={checked.has(d.id) || undefined}>
                <label className={adv.rowLabel}>
                  <Check checked={checked.has(d.id)} label={t("adv.check", { name: d.name })} onChange={(on) => checkWsl([d.id], on)} />
                  <span className={adv.rowText}>
                    <span className={adv.rowName}>
                      {d.name}
                      <span className={adv.tag}>{t(d.kind === "docker" ? "wsl.kind.docker" : "wsl.kind.wsl")}</span>
                    </span>
                    <span className={adv.rowPath}>{d.path}</span>
                  </span>
                  <span className={adv.rowMeta}>{t("wsl.vhdx")}</span>
                  <span className={adv.rowSize}>{formatBytes(d.size, t.locale)}</span>
                </label>
              </li>
            ))}
          </ul>
        ))}
      <SelectionBar
        summary={t.plural("wsl.selected", selected.length, { n: formatCount(selected.length, t.locale) })}
        onClear={() => checkWsl([...checked], false)}
        action={t("wsl.compact")}
        disabled={selected.length === 0}
        onAction={() => openRequest({ kind: "wslCompact", ids: selected.map((d) => d.id) })}
      />
    </>
  );
}

/** Projets (artefacts régénérables), caches de paquets, Docker, compactage WSL (Expérimental). */
export function DevScreen() {
  const t = useT();
  const [tab, setTab] = useState<DevTab>("projects");
  const scanId = useStore((s) => s.scan.summary?.scanId ?? null);
  const go = useStore((s) => s.go);
  const wslOn = useStore((s) => s.settings?.experimentalWsl ?? false);
  const openSettings = useStore((s) => s.setSettingsOpen);

  useEffect(() => {
    if (scanId !== null) ensureScanData("dev");
  }, [scanId]);

  const tabs: { value: DevTab; label: string }[] = [
    { value: "projects", label: t("dev.tab.projects") },
    { value: "caches", label: t("dev.tab.caches") },
    { value: "docker", label: t("dev.tab.docker") },
    { value: "wsl", label: t("dev.tab.wsl") },
  ];
  const needsScan = (tab === "projects" || tab === "caches") && scanId === null;
  return (
    <section aria-label={t("screen.dev.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.dev.title")}</h2>
          <p className={screens.subtitle}>{t("screen.dev.subtitle")}</p>
        </div>
        <Segmented<DevTab> options={tabs} value={tab} onChange={setTab} label={t("dev.tabs")} />
      </div>
      {needsScan ? (
        <div className={screens.empty}>
          <p>{t("dev.noScan")}</p>
          <Button variant="primary" onClick={() => go("analyse")}>
            {t("adv.toAnalyse")}
          </Button>
        </div>
      ) : tab === "wsl" && !wslOn ? (
        <div className={screens.empty}>
          <p>{t("wsl.off")}</p>
          <Button variant="primary" onClick={() => openSettings(true)}>
            {t("adv.system.settings")}
          </Button>
        </div>
      ) : (
        <div className={adv.tab}>
          {tab === "projects" && <ProjectsTab />}
          {tab === "caches" && <CachesTab />}
          {tab === "docker" && <DockerTab />}
          {tab === "wsl" && <WslTab />}
        </div>
      )}
    </section>
  );
}
