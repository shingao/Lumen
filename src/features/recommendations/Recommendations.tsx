import { useCopyFlash } from "../../app/clipboard";
import { useRecommendations } from "../../app/data";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { StatusDot } from "../../components/StatusDot";
import { formatBytes } from "../../core/format";
import { currentT, useT } from "../../i18n";
import { backend } from "../../ipc/client";
import type { Recommendation, RecoStatus } from "../../ipc/types";
import styles from "./Recommendations.module.css";

function Row({ reco }: { reco: Recommendation }) {
  const t = useT();
  const checked = useStore((s) => s.checked.includes(reco.id));
  const hovered = useStore((s) => s.hoverReco === reco.id);
  const toggle = useStore((s) => s.toggleChecked);
  const setHover = useStore((s) => s.setHoverReco);
  const open = useStore((s) => s.setOpenReco);
  return (
    <div
      className={styles.row}
      data-hover={hovered || undefined}
      onPointerEnter={() => setHover(reco.id)}
      onPointerLeave={() => setHover(null)}
      onFocus={() => setHover(reco.id)}
      onBlur={() => setHover(null)}
    >
      <label className={styles.label}>
        <input type="checkbox" className={styles.check} checked={checked} onChange={() => toggle(reco.id)} />
        <span className={styles.text}>
          <span className={styles.name}>{reco.name}</span>
          <span className={styles.path}>{reco.path}</span>
        </span>
        <span className={styles.size}>{formatBytes(reco.size, t.locale)}</span>
      </label>
      <button type="button" className={styles.chevron} aria-label={t("reco.detailLabel", { name: reco.name })} onClick={() => open(reco.id)}>
        ›
      </button>
    </div>
  );
}

function Group({ status, recos }: { status: RecoStatus; recos: Recommendation[] }) {
  const t = useT();
  const total = recos.reduce((s, r) => s + r.size, 0);
  return (
    <div className={styles.group}>
      <div className={styles.groupHead}>
        <span className={styles.groupTitle}>
          <StatusDot status={status} />
          {t(`status.${status}`)}
        </span>
        <span className={styles.groupTotal}>{formatBytes(total, t.locale)}</span>
      </div>
      <div className={styles.rows}>
        {recos.map((r) => (
          <Row key={r.id} reco={r} />
        ))}
      </div>
    </div>
  );
}

export function Recommendations() {
  const t = useT();
  const volume = useStore((s) => s.volume);
  const checked = useStore((s) => s.checked);
  const setChecked = useStore((s) => s.setChecked);
  const showToast = useStore((s) => s.showToast);
  const recos = useRecommendations(volume) ?? [];
  const [copied, copy] = useCopyFlash();

  const safe = recos.filter((r) => r.status === "safe");
  const review = recos.filter((r) => r.status === "review");
  const selected = recos.filter((r) => checked.includes(r.id));
  const selectedSize = selected.reduce((s, r) => s + r.size, 0);

  return (
    <div className={styles.column}>
      <div className={styles.header}>
        <h2 className={styles.title}>{t("reco.title")}</h2>
        <p className={styles.subtitle}>{t("reco.subtitle")}</p>
      </div>

      <div className={styles.body}>
        {recos.length === 0 && <p className={styles.empty}>{t("reco.empty")}</p>}
        {safe.length > 0 && <Group status="safe" recos={safe} />}
        {review.length > 0 && (
          <div className={styles.divider}>
            <Group status="review" recos={review} />
          </div>
        )}
      </div>

      <div className={styles.footer}>
        <span className={styles.summary}>
          {(() => {
            const [sizePart, rest] = t.plural("reco.selected", selected.length, { size: "\u0000" }).split("\u0000");
            return (
              <>
                {sizePart}
                <span className={styles.summarySize}>{formatBytes(selectedSize, t.locale)}</span>
                {rest}
              </>
            );
          })()}
        </span>
        <div className={styles.actions}>
          <Button
            variant="link"
            onClick={() => setChecked([...new Set([...checked, ...safe.map((r) => r.id)])])}
          >
            {t("reco.selectSafe")}
          </Button>
          <div className={styles.buttons}>
            <Button
              variant="primary"
              disabled={selected.length === 0}
              onClick={() =>
                void copy(backend.script(selected.map((r) => r.id), true)).then(
                  (ok) => ok && showToast(currentT()("preview.scriptCopied")),
                )
              }
            >
              {copied ? t("reco.copied") : t("reco.copyScript")}
            </Button>
            <Button variant="clean" disabled={selected.length === 0} onClick={() => showToast(currentT()("preview.later", { n: 5 }))}>
              {t("reco.clean")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
