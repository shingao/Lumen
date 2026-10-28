import { useEffect } from "react";
import { loadMoves, undoMove, unhealthy, useMoves } from "../../app/moves";
import { useStore } from "../../app/store";
import { Button } from "../../components/Button";
import { formatBytes, formatDate } from "../../core/format";
import { useT } from "../../i18n";
import type { MoveRow } from "../../ipc/types";
import styles from "./Moves.module.css";

function tone(r: MoveRow): "ok" | "warn" | undefined {
  if (r.state === "done") return r.health === "ok" ? "ok" : "warn";
  return r.state === "failed" ? "warn" : undefined;
}

/** Déplacements du journal : état, jonction suivie, « Annuler le déplacement ». */
export function MovesList() {
  const t = useT();
  const { rows, undoing } = useMoves();
  useEffect(() => {
    void loadMoves();
  }, []);
  if (rows === null) return null;
  return (
    <section className={styles.section} aria-label={t("move.list")}>
      <h3 className={styles.heading}>{t("move.list")}</h3>
      {rows.length === 0 ? (
        <p className={styles.paths}>
          <small>{t("move.listEmpty")}</small>
        </p>
      ) : (
        <ul className={styles.list}>
          {rows.map((r) => (
            <li key={r.id} className={styles.row}>
              <span className={styles.paths}>
                <span>{r.source}</span>
                <small>
                  → {r.target} · {formatDate(r.startedAt, t.locale)}
                  {r.message ? ` · ${r.message}` : ""}
                </small>
              </span>
              <span className={styles.size}>{formatBytes(r.bytes, t.locale)}</span>
              <span className={styles.badge} data-tone={tone(r)}>
                {r.state === "done" && r.health ? t(`move.health.${r.health}`) : t(`move.state.${r.state}`)}
              </span>
              {r.state === "done" && r.health === "ok" && (
                <Button variant="link" disabled={undoing !== null} onClick={() => void undoMove(r.id)}>
                  {undoing === r.id ? t("move.undoing") : t("move.undo")}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** Analyse : avertissement si un dossier déplacé par Lumen n'est plus joignable. */
export function MoveWarnings() {
  const t = useT();
  const rows = useMoves((s) => s.rows);
  const go = useStore((s) => s.go);
  useEffect(() => {
    void loadMoves();
  }, []);
  const bad = unhealthy(rows);
  if (bad.length === 0) return null;
  return (
    <div className={styles.banner} role="alert">
      <div>
        {t.plural("move.warning", bad.length)}
        <ul>
          {bad.map((r) => (
            <li key={r.id}>
              {r.source} → {r.target} : {r.health ? t(`move.health.${r.health}`) : ""}
            </li>
          ))}
        </ul>
      </div>
      <Button variant="link" onClick={() => go("journal")}>
        {t("move.warningSee")}
      </Button>
    </div>
  );
}
