import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { type StatusFocus, useStore } from "../../app/store";
import { useT } from "../../i18n";
import pop from "../../components/Popover.module.css";
import styles from "./StatusHelp.module.css";

/** Zones où un clic ne met pas fin à la mise en évidence : la treemap et sa navigation (zoom,
 * ← →, fil d'Ariane), les boutons « ? » et la bulle. */
export const KEEP_FOCUS = "data-status-focus-keep";

interface Props {
  status: StatusFocus["status"];
  from: StatusFocus["from"];
}

/** « ? » d'un statut : met en évidence ses tuiles dans la treemap (le reste estompé, comme au
 * survol d'une recommandation) et ouvre une bulle d'une phrase. Second clic, Échap ou clic
 * ailleurs : fin. Reste actif pendant le zoom et la navigation. */
export function StatusHelp({ status, from }: Props) {
  const t = useT();
  const focus = useStore((s) => s.statusFocus);
  const setFocus = useStore((s) => s.setStatusFocus);
  const ref = useRef<HTMLButtonElement>(null);
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  const active = focus?.status === status;
  const bubble = active && focus.from === from;

  useLayoutEffect(() => {
    if (!bubble) return;
    const place = () => {
      const box = ref.current?.getBoundingClientRect();
      if (box) setAnchor({ x: Math.round(box.left + box.width / 2), y: Math.round(box.bottom + 8) });
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [bubble]);

  useEffect(() => {
    if (!bubble) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setFocus(null);
      ref.current?.focus();
    };
    const onDown = (e: PointerEvent) => {
      const el = e.target instanceof Element ? e.target : null;
      if (!el?.closest(`[${KEEP_FOCUS}]`)) setFocus(null);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [bubble, setFocus]);

  return (
    <>
      <button
        ref={ref}
        type="button"
        className={styles.help}
        {...{ [KEEP_FOCUS]: "" }}
        aria-pressed={active}
        aria-label={t(`statusHelp.show.${status}`)}
        title={t(`statusHelp.show.${status}`)}
        onClick={(e) => {
          e.stopPropagation();
          setFocus(active && focus.from === from ? null : { status, from });
        }}
      >
        ?
      </button>
      {bubble &&
        anchor &&
        createPortal(
          <div role="note" aria-label={t(`status.${status}`)} className={`${pop.pop} ${styles.bubble}`} {...{ [KEEP_FOCUS]: "" }} style={{ left: anchor.x, top: anchor.y }}>
            {t(`statusHelp.text.${status}`)}
          </div>,
          document.body,
        )}
    </>
  );
}
