import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "../../components/Button";
import pop from "../../components/Popover.module.css";
import { useT } from "../../i18n";
import styles from "./FastScanConfirm.module.css";

interface Props {
  /** Bord droit et haut de la bulle (sous le bouton qui l'ouvre). */
  x: number;
  y: number;
  onConfirm(): void;
  onCancel(): void;
}

/** Une phrase d'explication avant la demande de droits administrateur du scan rapide. */
export function FastScanConfirm({ x, y, onConfirm, onCancel }: Props) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(x);

  useLayoutEffect(() => {
    const w = ref.current?.offsetWidth ?? 0;
    setLeft(Math.max(8, Math.min(x - w, window.innerWidth - w - 8)));
    ref.current?.querySelector("button")?.focus();
  }, [x]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onCancel();
    const onDown = (e: PointerEvent) => !ref.current?.contains(e.target as Node) && onCancel();
    window.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onDown, true);
    };
  }, [onCancel]);

  return createPortal(
    <div ref={ref} role="dialog" aria-label={t("explorer.fastScan")} className={`${pop.pop} ${styles.box}`} style={{ left, top: y }}>
      <p className={styles.text}>{t("explorer.fastScanText")}</p>
      <div className={styles.actions}>
        <Button variant="link" onClick={onCancel}>
          {t("explorer.fastScanCancel")}
        </Button>
        <Button variant="primary" onClick={onConfirm}>
          {t("explorer.fastScanGo")}
        </Button>
      </div>
    </div>,
    document.body,
  );
}
