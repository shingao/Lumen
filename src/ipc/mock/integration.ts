// Intégration Windows du backend factice : registre simulé (menu contextuel, démarrage), icône
// et notifications retenues pour les tests, demande « Analyser avec Lumen » simulée.
//
// Outils de dev (localStorage) : `lumen-dev-open` = chemin d'un dossier à ouvrir au démarrage
// (comme depuis l'Explorateur), `lumen-dev-integration` = état du registre simulé.

import type { ApplyIntegration, Integration, IntegrationEvent, OpenRequest } from "../types";

const REGISTRY_KEY = "lumen-dev-integration";
const OPEN_KEY = "lumen-dev-open";

interface Registry {
  explorerMenu: boolean;
  autostart: boolean;
}

/** Ce que l'interface a demandé (tests, captures). */
export interface MockIntegrationLog {
  applied: ApplyIntegration | null;
  notifications: { letter: string; title: string; body: string }[];
  errors: string[];
}

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

export function createIntegration(knownLetters: () => string[]) {
  // Installeur simulé : menu contextuel coché, démarrage non.
  let registry: Registry = { explorerMenu: true, autostart: false, ...read<Registry>(REGISTRY_KEY) };
  const listeners = new Set<(e: IntegrationEvent) => void>();
  const log: MockIntegrationLog = { applied: null, notifications: [], errors: [] };
  let pending: OpenRequest | null = null;

  /** Validation réduite à l'essentiel (la vraie est en Rust : canonisation, volumes). */
  const validate = (raw: string): OpenRequest => {
    const m = /^([A-Za-z]:)(\\.*)?$/.exec(raw.trim());
    if (!m) return { kind: "refused", reason: raw.startsWith("\\\\") ? "network" : "invalid" };
    const letter = (m[1] ?? "").toUpperCase();
    if (!knownLetters().includes(letter)) return { kind: "refused", reason: "volume" };
    const rest = (m[2] ?? "\\").replace(/\\+$/, "");
    return { kind: "open", letter, path: rest ? `${letter}${rest}` : `${letter}\\` };
  };

  try {
    const raw = localStorage.getItem(OPEN_KEY);
    if (raw) pending = validate(raw);
  } catch {
    // Pas de stockage : pas de demande simulée.
  }

  const save = () => {
    try {
      localStorage.setItem(REGISTRY_KEY, JSON.stringify(registry));
    } catch {
      // Non persistant : sans conséquence.
    }
  };
  const state = (): Integration => ({ ...registry, systemDrive: "C:", logs: "C:\\Users\\Arthur\\AppData\\Local\\Lumen\\logs", available: true });
  const emit = (e: IntegrationEvent) => listeners.forEach((l) => l(e));

  return {
    log,
    integration: () => Promise.resolve(state()),
    setExplorerMenu(on: boolean) {
      registry = { ...registry, explorerMenu: on };
      save();
      return Promise.resolve(state());
    },
    setAutostart(on: boolean) {
      registry = { ...registry, autostart: on };
      save();
      return Promise.resolve(state());
    },
    applyIntegration(request: ApplyIntegration) {
      log.applied = request;
      return Promise.resolve();
    },
    takeOpenRequest() {
      const r = pending;
      pending = null;
      return Promise.resolve(r);
    },
    onIntegrationEvent(listener: (e: IntegrationEvent) => void) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    notifyLowSpace(letter: string, title: string, body: string) {
      log.notifications.push({ letter, title, body });
      return Promise.resolve();
    },
    reportError(message: string) {
      log.errors.push(message);
    },
    openLogs: () => Promise.resolve(),
    /** Tests : second lancement depuis l'Explorateur, clic sur l'icône ou sur une notification. */
    simulate: {
      open(raw: string) {
        pending = validate(raw);
        emit({ type: "open" });
      },
      event: emit,
    },
  };
}

export type MockIntegration = ReturnType<typeof createIntegration>;
