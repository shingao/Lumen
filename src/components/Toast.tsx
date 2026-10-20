import { useEffect } from "react";
import { createPortal } from "react-dom";
import { useStore } from "../app/store";
import { useT } from "../i18n";
import { CloseSmallIcon } from "./icons";
import pop from "./Popover.module.css";
import styles from "./Toast.module.css";

const DURATION = 3200;

export function Toast() {
  const toast = useStore((s) => s.toast);
  const dismiss = useStore((s) => s.dismissToast);
  const t = useT();

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(dismiss, DURATION);
    return () => window.clearTimeout(timer);
  }, [toast, dismiss]);

  return createPortal(
    <div className={styles.region} role="status" aria-live="polite">
      {toast && (
        <div key={toast.id} className={`${pop.pop} ${styles.toast}`}>
          <span>{toast.text}</span>
          <button type="button" className={styles.close} aria-label={t("toast.close")} onClick={dismiss}>
            <CloseSmallIcon />
          </button>
        </div>
      )}
    </div>,
    document.body,
  );
}
