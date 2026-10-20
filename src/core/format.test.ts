import { describe, expect, it } from "vitest";
import { detectLocale, formatBytes, formatCount, formatDelta, formatRelative, GIB } from "./format";

const nbsp = (s: string) => s.replace(/[\u202f\u00a0]/g, " ");

describe("formatBytes (base 1024, comme l'Explorateur)", () => {
  it("français : virgule décimale et Ko/Mo/Go/To", () => {
    expect(formatBytes(13.4 * GIB, "fr")).toBe("13,4 Go");
    expect(formatBytes(204 * GIB, "fr")).toBe("204 Go");
    expect(formatBytes(1024 ** 4 * 1.5, "fr")).toBe("1,5 To");
    expect(formatBytes(1536, "fr")).toBe("1,5 Ko");
    expect(formatBytes(512, "fr")).toBe("512 o");
    expect(formatBytes(0.8 * GIB, "fr")).toBe("819 Mo");
  });

  it("anglais : point décimal et KB/MB/GB/TB", () => {
    expect(formatBytes(13.4 * GIB, "en")).toBe("13.4 GB");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
  });

  it("1 Go = 1024³ octets, pas 10⁹", () => {
    expect(formatBytes(1e9, "fr")).toBe("954 Mo");
    expect(formatBytes(GIB, "fr")).toBe("1,0 Go");
  });

  it("ne produit jamais « 1024 Mo »", () => {
    expect(formatBytes(GIB - 1, "fr")).toBe("1,0 Go");
  });
});

describe("autres formats", () => {
  it("deltas avec le vrai signe moins", () => {
    expect(formatDelta(-12.3 * GIB, "fr")).toBe("−12,3 Go");
    expect(formatDelta(44.2 * GIB, "en")).toBe("+44.2 GB");
  });

  it("nombres groupés", () => {
    expect(nbsp(formatCount(412380, "fr"))).toBe("412 380");
    expect(formatCount(412380, "en")).toBe("412,380");
  });

  it("dates relatives de la maquette", () => {
    const now = new Date(2026, 9, 20, 15, 0).getTime();
    expect(formatRelative(new Date(2026, 9, 20, 11, 42).getTime(), "fr", now)).toBe("aujourd'hui, 11:42");
    expect(formatRelative(new Date(2026, 9, 19, 22, 10).getTime(), "fr", now)).toBe("hier, 22:10");
    expect(formatRelative(now - 3 * 86_400_000, "fr", now)).toBe("il y a 3 jours");
    expect(formatRelative(now - 14 * 86_400_000, "fr", now)).toBe("il y a 2 semaines");
    expect(formatRelative(now - 240 * 86_400_000, "fr", now)).toBe("il y a 8 mois");
    expect(formatRelative(now - 3 * 86_400_000, "en", now)).toBe("3 days ago");
  });

  it("langue du système", () => {
    expect(detectLocale(["fr-FR", "en-US"])).toBe("fr");
    expect(detectLocale(["fr-CA"])).toBe("fr");
    expect(detectLocale(["en-GB", "fr-FR"])).toBe("en");
    expect(detectLocale(["de-DE"])).toBe("fr");
    expect(detectLocale([])).toBe("fr");
    expect(detectLocale(["de-DE", "fr-FR"])).toBe("fr");
  });
});
