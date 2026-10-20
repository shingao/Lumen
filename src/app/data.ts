// Accès aux données du backend depuis les composants : une ressource par clé, rechargée quand la clé change.

import { useEffect, useEffectEvent, useState } from "react";
import { backend } from "../ipc/client";
import { targetKey } from "../ipc/backend";
import type { Kpis, NodeId, Recommendation, RecommendationDetail, TreeSlice, Volume } from "../ipc/types";
import { useStore } from "./store";

/** Garde la dernière valeur pendant le chargement de la suivante (pas de clignotement au zoom). */
function useResource<T>(key: string | null, load: () => Promise<T>): T | undefined {
  const [state, setState] = useState<{ key: string; value: T } | null>(null);
  const run = useEffectEvent(load);
  useEffect(() => {
    if (key === null) return;
    let alive = true;
    run()
      .then((value) => {
        if (alive) setState({ key, value });
      })
      .catch((err: unknown) => console.error(`chargement « ${key} » :`, err));
    return () => {
      alive = false;
    };
  }, [key]);
  return key === null ? undefined : state?.value;
}

export function useVolumes(): Volume[] | undefined {
  const locale = useStore((s) => s.locale);
  return useResource(`volumes:${locale}`, () => backend.volumes());
}

/** Analyse terminée affichée (null tant qu'aucune n'est prête). */
function useDoneScanId(): number | null {
  return useStore((s) => s.scan.summary?.scanId ?? null);
}

export function useKpis(): Kpis | undefined {
  const target = useStore((s) => s.target);
  const summary = useStore((s) => s.scan.summary);
  return useResource(`kpis:${targetKey(target)}:${summary?.scanId ?? "none"}`, () => backend.kpis(target, summary));
}

export function useTree(node: NodeId | null): TreeSlice | undefined {
  const locale = useStore((s) => s.locale);
  const scanId = useDoneScanId();
  return useResource(scanId === null ? null : `tree:${scanId}:${node ?? "root"}:${locale}`, () => backend.tree(scanId ?? 0, node));
}

export function useRecommendations(): Recommendation[] | undefined {
  const locale = useStore((s) => s.locale);
  const scanId = useDoneScanId();
  return useResource(scanId === null ? null : `recos:${scanId}:${locale}`, () => backend.recommendations(scanId ?? 0));
}

export function useRecommendation(id: string | null): RecommendationDetail | undefined {
  const locale = useStore((s) => s.locale);
  const value = useResource(id === null ? null : `reco:${id}:${locale}`, () => backend.recommendation(id ?? ""));
  return value && value.id === id ? value : undefined;
}
