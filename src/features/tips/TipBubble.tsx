import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { hideTip, showTip, tipDue, useOnboarding, useTip } from "../../app/onboarding";
import { useStore } from "../../app/store";
import pop from "../../components/Popover.module.css";
import { CloseSmallIcon } from "../../components/icons";
import { useT } from "../../i18n";
import styles from "./TipBubble.module.css";

/** Position de la pointe dans la bulle : la bulle commence un peu avant le milieu de l'onglet. */
const ARROW_INSET = 20;

/**
 * Entoure la navigation : à la première visite d'un écran, une bulle sous son onglet explique
 * l'essentiel. Fermée en un clic, jamais répétée (mémorisée dès qu'elle s'affiche).
 */
export function TipAnchor({ children }: { children: ReactNode }) {
  const t = useT();
  const screen = useStore((s) => s.screen);
  const settings = useStore((s) => s.settings);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const onboardingOpen = useOnboarding((s) => s.open);
  const due = tipDue(screen, settings, onboardingOpen, settingsOpen);
  const tip = useTip((s) => s.screen);
  const [left, setLeft] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (due) showTip(screen);
    else if (tip !== null && tip !== screen) hideTip();
  }, [due, screen, tip]);

  const visible = tip === screen && !settingsOpen && !onboardingOpen;

  useLayoutEffect(() => {
    const tab = ref.current?.querySelector<HTMLElement>('[aria-current="page"]');
    if (tab) setLeft(tab.offsetLeft + tab.offsetWidth / 2);
  }, [visible, screen]);

  return (
    <div ref={ref} className={styles.anchor}>
      {children}
      {visible && (
        <div
          role="note"
          aria-label={t("tips.label")}
          className={`${pop.pop} ${styles.bubble}`}
          style={{ left: Math.max(0, left - ARROW_INSET), ["--tip-arrow" as string]: `${Math.min(left, ARROW_INSET) - 4}px` }}
          onClick={hideTip}
        >
          <span className={styles.text}>
            <span className={styles.label}>{t("tips.label")}</span>
            {t(`tips.${screen}`)}
          </span>
          <button type="button" className={styles.close} aria-label={t("tips.close")} onClick={hideTip}>
            <CloseSmallIcon />
          </button>
        </div>
      )}
    </div>
  );
}
