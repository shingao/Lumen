import { useEffect, useRef, useState } from "react";

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** « Copié » pendant --dur-flash après une copie réussie (comme la maquette). */
export function useCopyFlash(): [boolean, (text: Promise<string> | string) => Promise<boolean>] {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = async (text: Promise<string> | string) => {
    const ok = await copyText(await text);
    if (ok) {
      setCopied(true);
      window.clearTimeout(timer.current);
      const ms = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--dur-flash")) || 1600;
      timer.current = window.setTimeout(() => setCopied(false), ms);
    }
    return ok;
  };
  return [copied, copy];
}
