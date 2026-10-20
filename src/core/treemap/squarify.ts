// Treemap « squarified » (Bruls, Huizing, van Wijk 2000), sans dépendance.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Placed<T> {
  item: T;
  rect: Rect;
}

function worst(row: readonly number[], side: number): number {
  let sum = 0;
  let max = 0;
  let min = Infinity;
  for (const a of row) {
    sum += a;
    if (a > max) max = a;
    if (a < min) min = a;
  }
  const s2 = sum * sum;
  const side2 = side * side;
  return Math.max((side2 * max) / s2, s2 / (side2 * min));
}

/**
 * Place `items` dans `rect` avec des tuiles aussi carrées que possible.
 * `gap` : espace entre tuiles voisines ; les bords extérieurs touchent `rect`.
 * Les éléments de valeur nulle ou négative sont ignorés. Coordonnées arrondies au pixel.
 */
export function squarify<T>(items: readonly T[], value: (item: T) => number, rect: Rect, gap = 0): Placed<T>[] {
  const list = items.map((item) => ({ item, v: value(item) })).filter((e) => e.v > 0);
  list.sort((a, b) => b.v - a.v);
  if (list.length === 0 || rect.w <= 0 || rect.h <= 0) return [];

  // On travaille dans un rectangle élargi de gap/2, puis on rentre chaque tuile de gap/2.
  const half = gap / 2;
  let free: Rect = { x: rect.x - half, y: rect.y - half, w: rect.w + gap, h: rect.h + gap };
  const total = list.reduce((s, e) => s + e.v, 0);
  const scale = (free.w * free.h) / total;
  const areas = list.map((e) => e.v * scale);

  const raw: Rect[] = [];
  let row: number[] = [];

  const flush = () => {
    const sum = row.reduce((s, a) => s + a, 0);
    if (free.w >= free.h) {
      const colW = sum / free.h;
      let y = free.y;
      for (const a of row) {
        const h = a / colW;
        raw.push({ x: free.x, y, w: colW, h });
        y += h;
      }
      free = { x: free.x + colW, y: free.y, w: free.w - colW, h: free.h };
    } else {
      const rowH = sum / free.w;
      let x = free.x;
      for (const a of row) {
        const w = a / rowH;
        raw.push({ x, y: free.y, w, h: rowH });
        x += w;
      }
      free = { x: free.x, y: free.y + rowH, w: free.w, h: free.h - rowH };
    }
    row = [];
  };

  let i = 0;
  while (i < areas.length) {
    const a = areas[i] as number;
    const side = Math.min(free.w, free.h);
    if (row.length === 0 || worst(row, side) >= worst([...row, a], side)) {
      row.push(a);
      i++;
    } else {
      flush();
    }
  }
  if (row.length > 0) flush();

  return list.map((e, k) => {
    const r = raw[k] as Rect;
    const x0 = Math.round(r.x + half);
    const y0 = Math.round(r.y + half);
    const x1 = Math.round(r.x + r.w - half);
    const y1 = Math.round(r.y + r.h - half);
    return { item: e.item, rect: { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) } };
  });
}
