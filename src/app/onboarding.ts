// Tutoriel de bienvenue (4 écrans au plus) et astuces contextuelles (une par écran, une seule
// fois). Tout est mémorisé dans les réglages : `onboardingDone`, `tipsSeen`. La première
// analyse, lancée au démarrage, continue en arrière-plan pendant le tutoriel.

import { create } from "zustand";
import { currentT } from "../i18n";
import type { Settings } from "../ipc/types";
import type { Screen } from "./store";
import { useStore } from "./store";
import { updateSettings } from "./theme";

export const ONBOARDING_STEPS = 4;

export interface OnboardingState {
  open: boolean;
  /** 0 à ONBOARDING_STEPS - 1. */
  step: number;
}

export const useOnboarding = create<OnboardingState>()(() => ({ open: false, step: 0 }));

/** Astuce affichée (écran), jusqu'à ce qu'elle soit fermée ou que l'écran change. */
export const useTip = create<{ screen: Screen | null }>()(() => ({ screen: null }));

/** Montre l'astuce de l'écran et la mémorise aussitôt : elle ne reviendra jamais. */
export function showTip(screen: Screen): void {
  useTip.setState({ screen });
  void markTipSeen(screen);
}

export function hideTip(): void {
  useTip.setState({ screen: null });
}

/** Au démarrage : le tutoriel s'ouvre tant qu'il n'a été ni terminé ni passé. */
export function initOnboarding(settings: Settings | null): void {
  if (settings && !settings.onboardingDone) useOnboarding.setState({ open: true, step: 0 });
}

/** « Relancer le tutoriel » (Paramètres). */
export function openOnboarding(): void {
  useStore.getState().setSettingsOpen(false);
  useOnboarding.setState({ open: true, step: 0 });
}

export function nextStep(): void {
  const { step } = useOnboarding.getState();
  if (step + 1 >= ONBOARDING_STEPS) void finishOnboarding();
  else useOnboarding.setState({ step: step + 1 });
}

export function previousStep(): void {
  const { step } = useOnboarding.getState();
  if (step > 0) useOnboarding.setState({ step: step - 1 });
}

/** Fin, « Passer » ou Échap : fermé et jamais rouvert seul. */
export async function finishOnboarding(): Promise<void> {
  useOnboarding.setState({ open: false, step: 0 });
  if (!useStore.getState().settings?.onboardingDone) await updateSettings({ onboardingDone: true });
}

/** Astuce de l'écran à montrer maintenant (jamais pendant le tutoriel ni dans les Paramètres). */
export function tipDue(screen: Screen, settings: Settings | null, onboardingOpen: boolean, settingsOpen: boolean): boolean {
  return settings !== null && settings.onboardingDone && !onboardingOpen && !settingsOpen && !settings.tipsSeen.includes(screen);
}

/** Astuce montrée : elle ne le sera plus jamais (même si l'écran est quitté sans la fermer). */
export async function markTipSeen(screen: Screen): Promise<void> {
  const seen = useStore.getState().settings?.tipsSeen ?? [];
  if (!seen.includes(screen)) await updateSettings({ tipsSeen: [...seen, screen] });
}

/** « Réinitialiser les astuces » (Paramètres). */
export async function resetTips(): Promise<void> {
  await updateSettings({ tipsSeen: [] });
  useStore.getState().showToast(currentT()("tips.resetDone"));
}
