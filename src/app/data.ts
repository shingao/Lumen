// Accès aux données du backend depuis les composants : une ressource par clé, rechargée quand la clé change.

import { useEffect, useEffectEvent, useState } from "react";
import { backend } from "../ipc/client";
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

export function useKpis(volume: string): Kpis | undefined {
  return useResource(`kpis:${volume}`, () => backend.kpis(volume));
}

export function useTree(volume: string, node: NodeId | null): TreeSlice | undefined {
  const locale = useStore((s) => s.locale);
  return useResource(`tree:${volume}:${node ?? "root"}:${locale}`, () => backend.tree(volume, node));
}

export function useRecommendations(volume: string): Recommendation[] | undefined {
  const locale = useStore((s) => s.locale);
  return useResource(`recos:${volume}:${locale}`, () => backend.recommendations(volume));
}

export function useRecommendation(id: string | null): RecommendationDetail | undefined {
  const locale = useStore((s) => s.locale);
  const value = useResource(id === null ? null : `reco:${id}:${locale}`, () => backend.recommendation(id ?? ""));
  return value && value.id === id ? value : undefined;
}
