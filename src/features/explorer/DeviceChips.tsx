// Périphériques dans la barre de l'Explorer : une puce par volume (lettre, nom, type, espace
// utilisé en mini-jauge, « analysé il y a X »), tenue à jour à chaud. Un clic analyse le volume,
// ou affiche tout de suite sa dernière analyse gardée ; un lecteur réseau demande confirmation
// (lent). Les téléphones en MTP, sans lettre de lecteur, n'apparaissent pas.

import { useState } from "react";
import { useVolumes } from "../../app/data";
import { selectTarget } from "../../app/navigation";
import { useStore } from "../../app/store";
import { formatBytes, formatRelative } from "../../core/format";
import { useT } from "../../i18n";
import type { ScanTarget, Volume } from "../../ipc/types";
import { FastScanConfirm } from "./FastScanConfirm";
import styles from "./DeviceChips.module.css";

const same = (a: ScanTarget, b: ScanTarget) => JSON.stringify(a) === JSON.stringify(b);

export function DeviceChips() {
  const t = useT();
  const target = useStore((s) => s.target);
  const volumes = useVolumes() ?? [];
  const [network, setNetwork] = useState<{ volume: Volume; x: number; y: number } | null>(null);
  const user: ScanTarget = { kind: "userProfile" };

  const open = (v: Volume, el: HTMLElement) => {
    const to: ScanTarget = { kind: "volume", letter: v.letter };
    // Lecteur réseau : jamais analysé sans un clic confirmé (lent), sauf s'il l'a déjà été.
    if (v.kind === "network" && v.analyzedAt === null && !same(target, to)) {
      const box = el.getBoundingClientRect();
      setNetwork({ volume: v, x: box.right, y: box.bottom + 6 });
      return;
    }
    void selectTarget(to);
  };

  return (
    <div className={styles.row} role="group" aria-label={t("devices.label")}>
      <button type="button" className={styles.chip} aria-pressed={same(target, user)} onClick={() => void selectTarget(user)}>
        <span className={styles.top}>
          <span className={styles.name}>{t("explorer.target.user")}</span>
        </span>
        <span className={styles.meta}>{t("devices.userHint")}</span>
      </button>
      {volumes.map((v) => {
        const to: ScanTarget = { kind: "volume", letter: v.letter };
        const used = v.total > 0 ? (v.total - v.free) / v.total : 0;
        const kind = t(`devices.kind.${v.kind}`);
        return (
          <button
            key={v.key}
            type="button"
            className={styles.chip}
            data-kind={v.kind}
            aria-pressed={same(target, to)}
            aria-label={
              v.ready
                ? t("devices.chipLabel", { letter: v.letter, name: v.label || kind, kind, free: formatBytes(v.free, t.locale) })
                : t("devices.chipLabelPending", { letter: v.letter, kind })
            }
            title={v.ready ? t("explorer.driveFree", { free: formatBytes(v.free, t.locale), total: formatBytes(v.total, t.locale) }) : undefined}
            onClick={(e) => open(v, e.currentTarget)}
          >
            <span className={styles.top}>
              <span className={styles.letter}>{v.letter}</span>
              <span className={styles.name}>{v.label || kind}</span>
              <span className={styles.kind}>{kind}</span>
            </span>
            <span className={styles.gauge} aria-hidden="true">
              {v.ready && <span className={styles.gaugeFill} style={{ width: `${Math.min(100, Math.max(0, used * 100))}%` }} data-full={used >= 0.9 || undefined} />}
            </span>
            <span className={styles.meta}>
              {v.analyzedAt !== null
                ? t("devices.analyzed", { when: formatRelative(v.analyzedAt, t.locale) })
                : !v.ready
                  ? t(v.kind === "network" ? "devices.notRead" : "devices.reading")
                  : t("devices.free", { free: formatBytes(v.free, t.locale) })}
            </span>
          </button>
        );
      })}
      {network && (
        <FastScanConfirm
          x={network.x}
          y={network.y}
          label={t("devices.networkTitle", { letter: network.volume.letter })}
          text={t("devices.networkText", { letter: network.volume.letter })}
          go={t("devices.networkGo")}
          onCancel={() => setNetwork(null)}
          onConfirm={() => {
            const letter = network.volume.letter;
            setNetwork(null);
            void selectTarget({ kind: "volume", letter });
          }}
        />
      )}
    </div>
  );
}
