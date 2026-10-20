import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import styles from "./Popover.module.css";

export interface MenuItem {
  label: string;
  onSelect(): void;
  disabled?: boolean;
  /** Texte discret à droite (raccourci, jalon). */
  hint?: string;
  separatorBefore?: boolean;
}

interface Props {
  x: number;
  y: number;
  label: string;
  items: readonly MenuItem[];
  onClose(): void;
}

const MARGIN = 8;

/** Menu flottant (clic droit, sélecteur de disque) : clavier ↑ ↓ Entrée Échap, se ferme au clic extérieur. */
export function Menu({ x, y, label, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  const enabled = items.map((it, i) => (it.disabled ? -1 : i)).filter((i) => i >= 0);
  const [active, setActive] = useState(enabled[0] ?? -1);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(MARGIN, Math.min(x, window.innerWidth - width - MARGIN)),
      top: Math.max(MARGIN, Math.min(y, window.innerHeight - height - MARGIN)),
    });
    el.focus();
  }, [x, y]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onBlur = () => onClose();
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("blur", onBlur);
    window.addEventListener("resize", onBlur);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("resize", onBlur);
    };
  }, [onClose]);

  const move = (dir: 1 | -1) => {
    if (enabled.length === 0) return;
    const k = enabled.indexOf(active);
    setActive(enabled[(k + dir + enabled.length) % enabled.length] ?? -1);
  };

  return createPortal(
    <div
      ref={ref}
      role="menu"
      aria-label={label}
      tabIndex={-1}
      className={`${styles.pop} ${styles.menu}`}
      style={{ left: pos.left, top: pos.top }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") move(1);
        else if (e.key === "ArrowUp") move(-1);
        else if (e.key === "Escape" || e.key === "Tab") onClose();
        else if (e.key === "Enter" || e.key === " ") {
          const item = items[active];
          if (item && !item.disabled) {
            onClose();
            item.onSelect();
          }
        } else return;
        e.preventDefault();
      }}
    >
      {items.map((item, i) => (
        <Fragment key={item.label}>
          {item.separatorBefore && <div className={styles.separator} role="separator" />}
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            className={styles.menuItem}
            data-active={i === active || undefined}
            disabled={item.disabled}
            onPointerEnter={() => !item.disabled && setActive(i)}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            <span>{item.label}</span>
            {item.hint && <span className={styles.menuHint}>{item.hint}</span>}
          </button>
        </Fragment>
      ))}
    </div>,
    document.body,
  );
}
