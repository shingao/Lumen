import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import pop from "./Popover.module.css";
import styles from "./Modal.module.css";

interface Props {
  title: string;
  /** Échap ferme la fenêtre (sauf pendant une opération : `onClose` absent). */
  onClose?: () => void;
  /** Change quand le contenu change d'étape : le premier contrôle reprend le focus. */
  step?: string;
  children: ReactNode;
}

/** Fenêtre modale centrée : titre, contenu, focus sur le premier contrôle. */
export function Modal({ title, onClose, step, children }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("input, select, button:not(:disabled)")?.focus();
  }, [step]);

  return createPortal(
    <div className={styles.scrim}>
      <div ref={ref} role="dialog" aria-modal="true" aria-label={title} className={`${pop.pop} ${styles.dialog}`}>
        <h2 className={styles.title}>{title}</h2>
        {children}
      </div>
    </div>,
    document.body,
  );
}
