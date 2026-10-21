import { describe, expect, it } from "vitest";
import { canCheck, checkCopies, keepsOneEach, selectedBytes, toggleMember } from "./duplicates";

const group = { members: [{ id: 1, size: 10 }, { id: 2, size: 10 }, { id: 3, size: 10 }], keep: 1 };

describe("sélection des doublons", () => {
  it("jamais tous les exemplaires d'un groupe", () => {
    let checked = new Set<number>();
    checked = toggleMember(group, checked, 2);
    checked = toggleMember(group, checked, 3);
    expect(canCheck(group, checked, 1)).toBe(false);
    checked = toggleMember(group, checked, 1);
    expect([...checked].sort()).toEqual([2, 3]);
    expect(keepsOneEach([group], checked)).toBe(true);
    expect(keepsOneEach([group], new Set([1, 2, 3]))).toBe(false);
  });

  it("cocher les copies garde l'exemplaire présélectionné", () => {
    const checked = checkCopies(group, new Set([1]));
    expect([...checked].sort()).toEqual([2, 3]);
    expect(selectedBytes([group], checked)).toBe(20);
  });

  it("changer l'exemplaire gardé", () => {
    let checked = checkCopies(group, new Set());
    checked = toggleMember(group, checked, 2);
    checked = toggleMember(group, checked, 1);
    expect([...checked].sort()).toEqual([1, 3]);
  });
});
