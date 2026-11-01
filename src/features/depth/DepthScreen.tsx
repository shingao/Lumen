// Écran Profondeur : où passent les Go du volume système hors du dossier de l'utilisateur, en
// LECTURE SEULE. Postes classés par taille, chacun renvoie vers le module qui s'en occupe déjà
// (Gouffres système, Jeux, Dev, Paramètres de Windows) ; treemap du volume entier où la denylist
// est « Protégée ». Aucune case à cocher, aucune action de suppression.

import { useEffect, useRef, useState } from "react";
import { copyText } from "../../app/clipboard";
import { cancelDepth, loadCachedDepth, openDepthNode, readShadowStorage, startDepth, systemDrive, useDepth } from "../../app/depth";
import { showCachedScan, startScan } from "../../app/scanning";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes, formatCount, formatPercent, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import { backend } from "../../ipc/client";
import type { DepthEntry, DepthItem, DepthLink, DepthReport, NodeId } from "../../ipc/types";
import { Treemap } from "../explorer/Treemap";
import explorer from "../explorer/Explorer.module.css";
import screens from "../screens/Screens.module.css";
import styles from "./Depth.module.css";

/** Éléments montrés sous un poste avant « Tout afficher ». */
const SHOWN = 4;

export function DepthScreen() {
  const t = useT();
  const depth = useDepth();
  const fastAvailable = useStore((s) => s.appInfo?.fastScanAvailable ?? false);
  const mapRef = useRef<HTMLDivElement>(null);
  const volume = systemDrive();

  useEffect(() => {
    void loadCachedDepth();
  }, []);

  const running = depth.status === "running";
  const report = depth.report;
  const zoom = (node: NodeId | null) => {
    void openDepthNode(node);
    mapRef.current?.scrollIntoView?.({ behavior: "smooth", block: "start" });
  };

  return (
    <section aria-label={t("screen.depth.title")} className={`${screens.panel} ${screens.stub}`}>
      <div className={screens.header}>
        <div className={screens.heading}>
          <h2 className={screens.title}>{t("screen.depth.title")}</h2>
          <p className={screens.subtitle}>{t("screen.depth.subtitle")}</p>
        </div>
        {report && !running && <Actions fastAvailable={fastAvailable} again />}
      </div>

      {running && <Progress volume={volume} />}
      {depth.message && (
        <p role="alert" className={styles.error}>
          {t("depth.failed", { message: depth.message })}
        </p>
      )}

      {!report && !running && (
        <div className={screens.empty}>
          <p>{t("screen.depth.intro", { volume })}</p>
          <Actions fastAvailable={fastAvailable} again={false} />
          <p className={styles.hint}>{t("depth.walkHint")}</p>
        </div>
      )}

      {report && (
        <>
          <Status report={report} />
          <Items report={report} onOpen={zoom} />
          <div ref={mapRef} className={styles.map}>
            <MapBar />
            {depth.slice && (
              <Treemap
                root={depth.slice.root}
                highlight={null}
                variant="depth"
                onZoom={(id) => void openDepthNode(id)}
                onCopyPath={(node) => void copyText(node.path)}
                onUnavailable={() => undefined}
              />
            )}
            <div className={styles.legend}>
              <span className={styles.legendItem}>
                <span aria-hidden="true" className={styles.swatch} data-kind="protected" />
                {t("depth.legendProtected")}
              </span>
              <span className={styles.legendItem}>
                <span aria-hidden="true" className={styles.swatch} data-kind="other" />
                {t("depth.legendOther")}
              </span>
              <span className={styles.legendNote}>{t("depth.noAction")}</span>
            </div>
          </div>
        </>
      )}
    </section>
  );
}

function Actions({ fastAvailable, again }: { fastAvailable: boolean; again: boolean }) {
  const t = useT();
  return (
    <div className={styles.actions}>
      <Button variant={again ? "link" : "primary"} onClick={() => void startDepth("auto")}>
        {again ? t("depth.restart") : t("depth.start")}
      </Button>
      {fastAvailable && (
        <Button variant="link" title={t("depth.fastHint")} onClick={() => void startDepth("fast")}>
          {t("depth.fast")}
        </Button>
      )}
    </div>
  );
}

function Progress({ volume }: { volume: string }) {
  const t = useT();
  const progress = useDepth((s) => s.progress);
  return (
    <div className={styles.progress} role="status">
      <span className={styles.mono}>
        {t("depth.running", {
          volume,
          files: t.plural("explorer.files", progress?.files ?? 0, { n: formatCount(progress?.files ?? 0, t.locale) }),
          size: formatBytes(progress?.allocated ?? 0, t.locale),
        })}
      </span>
      <Button variant="link" onClick={() => void cancelDepth()}>
        {t("depth.cancel")}
      </Button>
    </div>
  );
}

function Status({ report }: { report: DepthReport }) {
  const t = useT();
  const parts = [t("depth.done", { volume: report.volume, when: formatRelative(report.finishedAt, t.locale) }), t(`depth.method.${report.method}`)];
  if (report.method === "walk" && report.accessDenied > 0) parts.push(t.plural("depth.denied", report.accessDenied, { n: formatCount(report.accessDenied, t.locale) }));
  if (report.used !== null) parts.push(t("depth.used", { size: formatBytes(report.used, t.locale) }));
  return <p className={styles.status}>{parts.join(" · ")}</p>;
}

function Items({ report, onOpen }: { report: DepthReport; onOpen(node: NodeId | null): void }) {
  const t = useT();
  const total = report.used ?? report.measured;
  return (
    <section aria-label={t("depth.items")}>
      <ul className={styles.items}>
        {report.items.map((item) => (
          <ItemRow key={item.id} item={item} report={report} total={total} onOpen={onOpen} />
        ))}
      </ul>
    </section>
  );
}

function ItemRow({ item, report, total, onOpen }: { item: DepthItem; report: DepthReport; total: number; onOpen(node: NodeId | null): void }) {
  const t = useT();
  const [all, setAll] = useState(false);
  const share = total > 0 ? item.size / total : 0;
  const about = item.id === "inaccessible" ? t(`depth.about.inaccessible.${report.method}`) : t(`depth.about.${item.id}`);
  const entries = all ? item.entries : item.entries.slice(0, SHOWN);
  return (
    <li className={styles.item} data-item={item.id}>
      <div className={styles.itemHead}>
        <span className={styles.itemName}>
          {item.id === "inaccessible" ? <StatusDot status="outline" /> : <StatusDot status={item.id === "userProfile" || item.id === "rootApps" ? "keep" : "protected"} />}
          {t(`depth.item.${item.id}`)}
        </span>
        <span className={styles.itemSize}>{formatBytes(item.size, t.locale)}</span>
      </div>
      <div className={styles.bar} role="img" aria-label={t("depth.share", { pct: formatPercent(share, t.locale) })}>
        <span className={styles.barFill} style={{ width: `${Math.min(100, share * 100)}%` }} data-item={item.id} />
      </div>
      <p className={styles.about}>{about}</p>
      {entries.length > 0 && (
        <ul className={styles.entries}>
          {entries.map((e) => (
            <EntryRow key={`${e.path}-${e.name}`} entry={e} onOpen={onOpen} />
          ))}
        </ul>
      )}
      <div className={styles.itemFoot}>
        {item.entries.length > SHOWN && (
          <Button variant="link" onClick={() => setAll(!all)}>
            {all ? t("depth.less") : t("depth.more", { n: item.entries.length })}
          </Button>
        )}
        {item.id === "restore" && <ShadowStorage volume={report.volume} />}
        {item.link && <ModuleLink link={item.link} />}
      </div>
    </li>
  );
}

function EntryRow({ entry, onOpen }: { entry: DepthEntry; onOpen(node: NodeId | null): void }) {
  const t = useT();
  const name = entry.name || t("depth.restOfWindows");
  return (
    <li className={styles.entry}>
      {entry.node !== null ? (
        <button type="button" className={styles.entryName} title={t("depth.openNode", { name })} onClick={() => onOpen(entry.node)}>
          {name}
        </button>
      ) : (
        <span className={styles.entryText}>{name}</span>
      )}
      <code className={styles.entryPath}>{entry.path}</code>
      <span className={styles.entrySize}>{formatBytes(entry.size, t.locale)}</span>
    </li>
  );
}

/** Espace des clichés instantanés, lu en administrateur sur demande (lecture seule). */
function ShadowStorage({ volume }: { volume: string }) {
  const t = useT();
  const restore = useDepth((s) => s.restore);
  if (restore.status === "done" && restore.data) {
    const s = restore.data.storage;
    return (
      <span className={styles.shadow}>
        {s ? t("depth.shadow.value", { used: formatBytes(s.used, t.locale), max: formatBytes(s.max, t.locale) }) : t("depth.shadow.none", { volume })}
      </span>
    );
  }
  return (
    <>
      {restore.status === "failed" && <span className={styles.shadow}>{t("depth.shadow.failed", { message: restore.message ?? "" })}</span>}
      <Button variant="link" disabled={restore.status === "loading"} onClick={() => void readShadowStorage()}>
        {restore.status === "loading" ? t("depth.shadow.reading") : t("depth.shadow.read")}
      </Button>
    </>
  );
}

/** Renvoi vers le module qui s'occupe déjà du poste ; jamais une action ici. */
function ModuleLink({ link }: { link: DepthLink }) {
  const t = useT();
  const go = useStore((s) => s.go);
  const setCleanupTab = useStore((s) => s.setCleanupTab);
  const open = () => {
    switch (link) {
      case "sinks":
        setCleanupTab("system");
        go("cleanup");
        break;
      case "games":
        go("games");
        break;
      case "dev":
        go("dev");
        break;
      case "uninstall":
        void backend.openAppsSettings().catch((err: unknown) => console.error("paramètres :", err));
        break;
      case "analysis": {
        const target = { kind: "userProfile" } as const;
        useStore.getState().setTarget(target);
        go("analyse");
        void showCachedScan(target).then((id) => {
          if (id === null) void startScan(target);
        });
        break;
      }
    }
  };
  return (
    <Button variant="link" className={styles.link} onClick={open}>
      {t(`depth.link.${link}`)}
      {link !== "uninstall" && <span aria-hidden="true"> →</span>}
    </Button>
  );
}

function MapBar() {
  const t = useT();
  const slice = useDepth((s) => s.slice);
  const crumbs = slice?.breadcrumb ?? [];
  return (
    <div className={explorer.crumbBar}>
      <h3 className={styles.mapTitle}>{t("depth.treemap")}</h3>
      <nav aria-label={t("explorer.breadcrumb")} className={explorer.crumbs}>
        {crumbs.map((c, i) => {
          const last = i === crumbs.length - 1;
          return (
            <span key={`${c.label}-${i}`} className={explorer.crumbItem}>
              {i > 0 && (
                <span aria-hidden="true" className={explorer.sep}>
                  \
                </span>
              )}
              {c.id !== null && !last ? (
                <button type="button" className={explorer.crumbLink} onClick={() => void openDepthNode(c.id === 0 ? null : c.id)}>
                  {c.label}
                </button>
              ) : (
                <span className={last ? explorer.current : undefined} aria-current={last ? "location" : undefined}>
                  {c.label}
                </span>
              )}
            </span>
          );
        })}
      </nav>
      <div className={explorer.crumbEnd}>
        <span className={explorer.total}>{slice ? formatBytes(slice.root.size, t.locale) : ""}</span>
      </div>
    </div>
  );
}
