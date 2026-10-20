import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { commandList, type Command } from "../../app/commands";
import { useStore } from "../../app/store";
import { SearchIcon } from "../../components/icons";
import pop from "../../components/Popover.module.css";
import { formatBytes, formatRelative } from "../../core/format";
import { fuzzyFilter } from "../../core/fuzzy";
import { currentT, useT } from "../../i18n";
import { backend } from "../../ipc/client";
import type { SearchHit } from "../../ipc/types";
import styles from "./Palette.module.css";

type Entry = { kind: "command"; command: Command } | { kind: "hit"; hit: SearchHit };

/** Palette Ctrl K : commandes et recherche dans l'index du dernier scan. */
export function Palette() {
  const t = useT();
  const close = () => useStore.getState().setPaletteOpen(false);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [active, setActive] = useState(0);
  const [now] = useState(() => Date.now());
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const restoreFocus = useRef<Element | null>(null);

  useEffect(() => {
    restoreFocus.current = document.activeElement;
    inputRef.current?.focus();
    return () => {
      if (restoreFocus.current instanceof HTMLElement) restoreFocus.current.focus();
    };
  }, []);

  useEffect(() => {
    let alive = true;
    const q = query.trim();
    const pending = q.length === 0 ? Promise.resolve([]) : backend.search(q, 8);
    void pending.then((h) => {
      if (alive) setHits(h);
    });
    return () => {
      alive = false;
    };
  }, [query]);

  const commands = useMemo(() => {
    const all = commandList();
    return query.trim() ? fuzzyFilter(all, query, (c) => c.label, 6) : all.slice(0, 6);
  }, [query]);

  const entries: Entry[] = [
    ...commands.map((command) => ({ kind: "command" as const, command })),
    ...hits.map((hit) => ({ kind: "hit" as const, hit })),
  ];
  const current = Math.min(active, Math.max(0, entries.length - 1));

  useEffect(() => {
    listRef.current?.querySelector("[data-active]")?.scrollIntoView({ block: "nearest" });
  }, [current]);

  // Entrée ouvre le fichier, Ctrl+Entrée le montre dans l'Explorateur : tous deux arrivent avec le scanner.
  const run = (entry: Entry | undefined) => {
    if (!entry) return;
    close();
    if (entry.kind === "command") entry.command.run();
    else useStore.getState().showToast(currentT()("preview.later", { n: 3 }));
  };

  return createPortal(
    <div className={styles.scrim} onPointerDown={(e) => e.target === e.currentTarget && close()}>
      <div role="dialog" aria-modal="true" aria-label={t("palette.label")} className={`${pop.pop} ${styles.palette}`}>
        <div className={styles.field}>
          <span className={styles.icon}>
            <SearchIcon size={14} />
          </span>
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            placeholder={t("palette.placeholder")}
            aria-label={t("palette.placeholder")}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={entries.length ? `palette-${current}` : undefined}
            onChange={(e) => {
              setQuery(e.target.value);
              setActive(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") setActive((current + 1) % Math.max(1, entries.length));
              else if (e.key === "ArrowUp") setActive((current - 1 + entries.length) % Math.max(1, entries.length));
              else if (e.key === "Enter") run(entries[current]);
              else if (e.key === "Escape") close();
              else return;
              e.preventDefault();
            }}
          />
        </div>

        <div ref={listRef} id="palette-list" role="listbox" aria-label={t("palette.label")} className={styles.list}>
          {commands.length > 0 && <div className={styles.group}>{t("palette.commands")}</div>}
          {entries.map((entry, i) => {
            const isFirstHit = entry.kind === "hit" && i === commands.length;
            return (
              <div key={entry.kind === "command" ? entry.command.id : `hit-${entry.hit.id}`}>
                {isFirstHit && <div className={styles.group}>{t("palette.files")}</div>}
                <div
                  id={`palette-${i}`}
                  role="option"
                  aria-selected={i === current}
                  data-active={i === current || undefined}
                  className={styles.option}
                  onPointerMove={() => i !== current && setActive(i)}
                  onClick={() => run(entry)}
                >
                  {entry.kind === "command" ? (
                    <>
                      <span className={styles.label}>{entry.command.label}</span>
                      {entry.command.hint && <span className={styles.meta}>{entry.command.hint}</span>}
                    </>
                  ) : (
                    <>
                      <span className={styles.hitText}>
                        <span className={styles.label}>{entry.hit.name}</span>
                        <span className={styles.path}>{entry.hit.path}</span>
                      </span>
                      <span className={styles.hitMeta}>
                        <span className={styles.size}>{formatBytes(entry.hit.size, t.locale)}</span>
                        <span className={styles.meta}>{formatRelative(entry.hit.modified, t.locale, now)}</span>
                      </span>
                    </>
                  )}
                </div>
              </div>
            );
          })}
          {entries.length === 0 && <div className={styles.none}>{t("palette.none")}</div>}
        </div>

        <div className={styles.footer}>
          <span>
            <kbd className={styles.kbd}>↑↓</kbd> {t("palette.hintMove")}
          </span>
          <span>
            <kbd className={styles.kbd}>{t("palette.keyEnter")}</kbd> {t("palette.hintOpen")}
          </span>
          <span>
            <kbd className={styles.kbd}>{t("palette.keyCtrlEnter")}</kbd> {t("palette.hintReveal")}
          </span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
