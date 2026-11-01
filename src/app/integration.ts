// Intégration Windows côté interface (jalon 11) : demandes « Analyser avec Lumen », icône de la
// zone de notification (infobulle de l'espace libre, menu), notifications d'espace libre, menu
// contextuel et démarrage (registre de l'utilisateur, lu par `integration()`), erreurs de
// l'interface vers le journal local.

import { create } from "zustand";
import { dueAlerts, freeRatio, localDay, markSent } from "../core/alerts";
import { formatBytes, formatPercent } from "../core/format";
import { currentT } from "../i18n";
import { backend } from "../ipc/client";
import type { Integration, OpenRequest, Volume } from "../ipc/types";
import { goToTarget, openFolder } from "./navigation";
import { useStore } from "./store";
import { updateSettings } from "./theme";

/** Espace libre relu (infobulle, alertes) à cet intervalle. */
const REFRESH_MS = 10 * 60 * 1000;

export const useIntegration = create<{ info: Integration | null }>()(() => ({ info: null }));

let started = false;
let timer: ReturnType<typeof setInterval> | null = null;
let offs: (() => void)[] = [];

async function refreshInfo(): Promise<Integration | null> {
  const info = await backend.integration().catch(() => null);
  useIntegration.setState({ info });
  return info;
}

/** Infobulle de l'icône : espace libre du disque principal. */
export function trayTooltip(volumes: readonly Volume[], systemDrive: string): string {
  const t = currentT();
  const v = volumes.find((x) => x.letter.toUpperCase() === systemDrive.toUpperCase());
  if (!v) return "Lumen";
  return t("integration.tooltip", { free: formatBytes(v.free, t.locale), total: formatBytes(v.total, t.locale), drive: v.letter });
}

/** Icône, libellés et infobulle transmis au backend (au démarrage, à chaque réglage ou langue). */
export async function applyIntegration(volumes?: readonly Volume[]): Promise<void> {
  const settings = useStore.getState().settings;
  if (!settings) return;
  const t = currentT();
  const info = useIntegration.getState().info;
  const drive = info?.systemDrive ?? "C:";
  const list = volumes ?? (await backend.volumes().catch(() => []));
  await backend
    .applyIntegration({
      tray: settings.trayIcon,
      tooltip: trayTooltip(list, drive),
      labels: { open: t("integration.trayOpen"), scan: t("integration.trayScan", { drive }), quit: t("integration.trayQuit") },
      menuLabel: t("integration.menuLabel"),
    })
    .catch((err: unknown) => console.error("zone de notification :", err));
}

/** Notifications du jour pour les disques sous le seuil (au plus une par jour et par disque). */
export async function checkLowSpace(volumes: readonly Volume[], now = Date.now()): Promise<number> {
  const settings = useStore.getState().settings;
  if (!settings?.lowSpaceAlert) return 0;
  const today = localDay(now);
  const due = dueAlerts(volumes, settings.lowSpaceThreshold, settings.alertsSent, today);
  if (due.length === 0) return 0;
  const t = currentT();
  const sent: string[] = [];
  for (const v of due) {
    const title = t("integration.alertTitle", { drive: v.letter });
    const body = t("integration.alertBody", { free: formatBytes(v.free, t.locale), percent: formatPercent(freeRatio(v), t.locale) });
    try {
      await backend.notifyLowSpace(v.letter, title, body);
      sent.push(v.key);
    } catch (err) {
      console.error("notification :", err);
    }
  }
  if (sent.length > 0) await updateSettings({ alertsSent: markSent(settings.alertsSent, sent, today) });
  return sent.length;
}

/** Espace libre relu : infobulle de l'icône, puis alertes. */
export async function tick(now = Date.now()): Promise<void> {
  const volumes = await backend.volumes().catch(() => []);
  await applyIntegration(volumes);
  await checkLowSpace(volumes, now);
}

/** Demande du menu contextuel : Explorer sur le dossier, ou refus expliqué. */
export async function handleOpen(request: OpenRequest | null): Promise<void> {
  if (!request) return;
  const t = currentT();
  if (request.kind === "refused") {
    useStore.getState().showToast(t("integration.refused", { reason: t(`integration.refused.${request.reason}`) }));
    return;
  }
  const found = await openFolder(request.letter, request.path);
  if (!found) useStore.getState().showToast(t("integration.notFound", { drive: request.letter }));
}

export async function setExplorerMenu(on: boolean): Promise<void> {
  const info = await backend.setExplorerMenu(on, currentT()("integration.menuLabel"));
  useIntegration.setState({ info });
}

export async function setAutostart(on: boolean): Promise<void> {
  const info = await backend.setAutostart(on);
  useIntegration.setState({ info });
}

/** Erreurs non rattrapées de l'interface : journal local. */
function reportErrors(): void {
  window.addEventListener("error", (e) => backend.reportError(`${e.message}\n${e.error instanceof Error ? (e.error.stack ?? "") : ""}`));
  window.addEventListener("unhandledrejection", (e) => {
    const r: unknown = e.reason;
    backend.reportError(`promesse rejetée : ${r instanceof Error ? `${r.message}\n${r.stack ?? ""}` : String(r)}`);
  });
}

/** Démarrage : état du registre, événements, demande du premier lancement (renvoyée pour que
 * l'analyse du démarrage s'y range), icône et alertes. */
export async function initIntegration(): Promise<OpenRequest | null> {
  if (started) return null;
  started = true;
  reportErrors();
  await refreshInfo();
  const offEvents = backend.onIntegrationEvent((e) => {
    if (e.type === "open") void backend.takeOpenRequest().then(handleOpen);
    else if (e.type === "alert") void openFolder(e.letter, `${e.letter}\\`);
    else if (useStore.getState().scan.status !== "running") {
      const s = useStore.getState();
      s.setSettingsOpen(false);
      s.go("analyse");
      goToTarget({ kind: "volume", letter: e.letter });
    }
  });
  // Icône et libellés suivent les réglages et la langue.
  const offStore = useStore.subscribe((s, prev) => {
    if (s.settings?.trayIcon !== prev.settings?.trayIcon || s.locale !== prev.locale) void applyIntegration();
    if (s.settings?.lowSpaceAlert && (!prev.settings?.lowSpaceAlert || s.settings.lowSpaceThreshold !== prev.settings.lowSpaceThreshold)) void tick();
  });
  offs = [offEvents, offStore];
  const request = await backend.takeOpenRequest().catch(() => null);
  void tick();
  timer ??= setInterval(() => void tick(), REFRESH_MS);
  return request;
}

/** Pour les tests. */
export function resetIntegration(): void {
  started = false;
  offs.forEach((off) => off());
  offs = [];
  if (timer) clearInterval(timer);
  timer = null;
  useIntegration.setState({ info: null });
}
