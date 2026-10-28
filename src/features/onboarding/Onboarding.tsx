import { useEffect, useMemo, useRef } from "react";
import { finishOnboarding, nextStep, ONBOARDING_STEPS, previousStep, useOnboarding } from "../../app/onboarding";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { CheckIcon, LogoIcon } from "../../components/icons";
import { formatBytes, formatCount, GIB } from "../../core/format";
import { squarify } from "../../core/treemap/squarify";
import { useT } from "../../i18n";
import type { Category } from "../../ipc/types";
import styles from "./Onboarding.module.css";

interface DemoTile {
  key: string;
  category: Category;
  /** Go (illustration, pas les données de l'utilisateur). */
  size: number;
  safe: boolean;
}

/** Disque d'exemple : quelques catégories, trois caches sans risque. */
const DEMO: DemoTile[] = [
  { key: "media", category: "media", size: 26, safe: false },
  { key: "code", category: "code", size: 18, safe: false },
  { key: "android", category: "android", size: 17, safe: false },
  { key: "ai", category: "ai", size: 16, safe: false },
  { key: "npm", category: "packages", size: 13.4, safe: true },
  { key: "games", category: "games", size: 12, safe: false },
  { key: "gradle", category: "packages", size: 10.6, safe: true },
  { key: "temp", category: "tools", size: 4.1, safe: true },
];

const W = 600;
const H = 300;
const pct = (v: number, of: number) => `${((v / of) * 100).toFixed(3)}%`;

/** Tutoriel de bienvenue : 4 écrans, « Passer » toujours visible, Échap ferme, flèches pour naviguer. */
export function Onboarding() {
  const t = useT();
  const step = useOnboarding((s) => s.step);
  const scan = useStore((s) => s.scan);
  const ref = useRef<HTMLDivElement>(null);
  const tiles = useMemo(() => squarify(DEMO, (d) => d.size, { x: 0, y: 0, w: W, h: H }, 3), []);
  const safeTotal = DEMO.filter((d) => d.safe).reduce((n, d) => n + d.size, 0);
  const last = step === ONBOARDING_STEPS - 1;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void finishOnboarding();
      else if (e.key === "ArrowRight") nextStep();
      else if (e.key === "ArrowLeft") previousStep();
      else if (e.key === "Tab") {
        // Focus gardé dans le tutoriel.
        const items = [...(ref.current?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
        const first = items[0];
        const end = items[items.length - 1];
        if (!first || !end) return;
        if (e.shiftKey && document.activeElement === first) end.focus();
        else if (!e.shiftKey && document.activeElement === end) first.focus();
        else return;
      } else return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("[data-primary]")?.focus();
  }, [step]);

  const status =
    scan.status === "done"
      ? t("onboarding.scanReady")
      : scan.status === "running"
        ? t("onboarding.scanRunning", { files: formatCount(scan.progress?.files ?? 0, t.locale) })
        : null;

  return (
    <div className={styles.screen}>
      <div ref={ref} className={styles.card} role="dialog" aria-modal="true" aria-labelledby="onboarding-phrase" aria-describedby="onboarding-step">
        <div className={styles.top}>
          <span className={styles.brand}>
            <LogoIcon />
            {t("onboarding.welcome")}
          </span>
          <Button variant="link" onClick={() => void finishOnboarding()}>
            {t("onboarding.skip")}
          </Button>
        </div>

        <div className={styles.map} data-step={step} aria-hidden="true">
          {tiles.map(({ item, rect }, i) => (
            <div
              key={item.key}
              className={styles.tile}
              data-safe={item.safe || undefined}
              style={{
                left: pct(rect.x, W),
                top: pct(rect.y, H),
                width: pct(rect.w, W),
                height: pct(rect.h, H),
                backgroundColor: `var(--cat-${item.category})`,
                animationDelay: `calc(var(--dur-stagger) * ${i})`,
              }}
            >
              {item.safe && <div className={styles.hatch} />}
              {rect.w > 90 && rect.h > 44 && (
                <>
                  <span>{t(`onboarding.demo.${item.key}` as "onboarding.demo.media")}</span>
                  <span className={styles.size}>{formatBytes(item.size * GIB, t.locale)}</span>
                </>
              )}
              {item.safe && (
                <div className={styles.check}>
                  <CheckIcon />
                </div>
              )}
            </div>
          ))}
        </div>

        {/* Place réservée dès le 1er écran : le conteneur ne bouge pas quand la pastille apparaît. */}
        <span className={styles.chip} data-hidden={step < 2 || undefined} aria-hidden={step < 2 || undefined}>
          {t(last ? "onboarding.chipTrash" : "onboarding.chipChecked", { n: DEMO.filter((d) => d.safe).length })}{" "}
          <b>{formatBytes(safeTotal * GIB, t.locale)}</b>
        </span>

        <p id="onboarding-phrase" className={styles.phrase} aria-live="polite">
          {t(`onboarding.step${step + 1}` as "onboarding.step1")}
        </p>

        <div className={styles.bottom}>
          <div className={styles.dots}>
            <span id="onboarding-step" className={styles.status}>
              {t("onboarding.progress", { n: step + 1, total: ONBOARDING_STEPS })}
            </span>
            {Array.from({ length: ONBOARDING_STEPS }, (_, i) => (
              <span key={i} className={styles.dot} data-on={i === step || undefined} aria-hidden="true" />
            ))}
          </div>
          <div className={styles.actions}>
            {step > 0 && (
              <Button variant="link" onClick={previousStep}>
                {t("onboarding.previous")}
              </Button>
            )}
            <Button variant="primary" data-primary onClick={nextStep}>
              {last ? t("onboarding.start") : t("onboarding.next")}
            </Button>
          </div>
        </div>
        {status && (
          <span className={styles.status} role="status">
            {status}
          </span>
        )}
      </div>
    </div>
  );
}
