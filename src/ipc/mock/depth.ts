// Analyse en profondeur factice du volume C: (navigateur) : postes et treemap du volume entier,
// zones de la denylist marquées « Protégées ».

import { GIB } from "../../core/format";
import type { Category, Crumb, DepthEntry, DepthItem, DepthReport, NodeId, TreeNode, TreeSlice } from "../types";

const gb = (n: number) => Math.round(n * GIB);
const DAY = 86_400_000;

interface Spec {
  name: string;
  size: number;
  protected?: boolean;
  category?: Category;
  /** Profil d'un autre compte : jamais détaillé. */
  closed?: boolean;
  children?: Spec[];
}

/** Volume C: (Go) : occupé 402,2 Go ; ce que le parcours sans droits n'a pas lu va dans « Inaccessible ». */
const VOLUME: Spec[] = [
  {
    name: "Windows",
    size: 38.4,
    protected: true,
    children: [
      {
        name: "WinSxS",
        size: 9.8,
        protected: true,
        children: [
          { name: "Backup", size: 1.2, protected: true },
          { name: "amd64_microsoft-windows-servicingstack", size: 2.6, protected: true },
          { name: "Temp", size: 0.6, protected: true },
        ],
      },
      { name: "System32", size: 9.5, protected: true, children: [{ name: "DriverStore", size: 3.4, protected: true }] },
      { name: "SoftwareDistribution", size: 6.2, protected: true },
      { name: "Installer", size: 5.1, protected: true },
      { name: "SysWOW64", size: 2.1, protected: true },
    ],
  },
  {
    name: "Program Files",
    size: 58.6,
    protected: true,
    children: [
      { name: "Microsoft Visual Studio", size: 18.2, protected: true, category: "tools" },
      { name: "Adobe", size: 12.4, protected: true, category: "media" },
      { name: "WindowsApps", size: 7.3, protected: true },
      { name: "NVIDIA Corporation", size: 2.6, protected: true },
      { name: "Microsoft Office", size: 4.1, protected: true },
      { name: "Docker", size: 3.1, protected: true, category: "tools" },
    ],
  },
  { name: "Program Files (x86)", size: 13.0, protected: true, children: [{ name: "Steam", size: 9.6, protected: true, category: "games" }, { name: "Microsoft", size: 2.4, protected: true }] },
  {
    name: "ProgramData",
    size: 22.4,
    protected: true,
    children: [
      { name: "DockerDesktop", size: 14.2, protected: true, category: "tools" },
      { name: "Microsoft", size: 4.6, protected: true },
      { name: "Package Cache", size: 2.8, protected: true },
    ],
  },
  {
    name: "Users",
    size: 116.3,
    children: [
      {
        name: "arthur",
        size: 84.7,
        children: [
          { name: "AppData", size: 41.2, category: "packages" },
          { name: "Documents", size: 18.6, category: "media" },
          { name: "Projets", size: 16.3, category: "code" },
          { name: "Vidéos", size: 6.2, category: "media" },
        ],
      },
      { name: "Admin", size: 31.2, protected: true, closed: true },
      { name: "Public", size: 0.4, protected: true },
    ],
  },
  { name: "Riot Games", size: 46.5, category: "games", children: [{ name: "League of Legends", size: 27.9, category: "games" }, { name: "VALORANT", size: 18.6, category: "games" }] },
  { name: "XboxGames", size: 41.3, protected: true, category: "games", children: [{ name: "Halo Infinite", size: 41.3, protected: true, category: "games" }] },
  { name: "Android", size: 18.1, category: "android" },
  { name: "hiberfil.sys", size: 12.7, protected: true },
  { name: "pagefile.sys", size: 8, protected: true },
  { name: "System Volume Information", size: 7.4, protected: true },
  { name: "$Recycle.Bin", size: 1.9, protected: true },
  { name: "swapfile.sys", size: 0.25, protected: true },
];

const USED = 402.2;
const ROOT = "C:\\";

interface Built {
  node: TreeNode;
  parent: NodeId | null;
}

let tree: Map<NodeId, Built> | null = null;

function build(): Map<NodeId, Built> {
  const map = new Map<NodeId, Built>();
  let next = 0;
  const make = (spec: Spec, path: string, parent: NodeId | null): TreeNode => {
    const id = next++;
    const isFile = /\.sys$/.test(spec.name);
    const node: TreeNode = {
      id,
      name: spec.name,
      path,
      kind: isFile ? "file" : "dir",
      size: gb(spec.size),
      logical: gb(spec.size),
      othersCount: null,
      files: isFile ? 1 : Math.round(spec.size * 4_100),
      lastOpened: Date.now() - 3 * DAY,
      category: spec.category ?? "system",
      status: "keep",
      reco: null,
      gain: 0,
      gainStatus: null,
      gainUpTo: false,
      coverDepth: null,
      opaque: null,
      ...(spec.protected ? { protected: true } : {}),
      hasChildren: !spec.closed && (spec.children?.length ?? 0) > 0,
      children: [],
    };
    map.set(id, { node, parent });
    if (!spec.closed) node.children = (spec.children ?? []).map((c) => make(c, `${path}\\${c.name}`, id));
    return node;
  };
  const measured = VOLUME.reduce((s, v) => s + v.size, 0);
  make({ name: "C:", size: measured, children: VOLUME }, "C:", null);
  // Chemins sous la racine : « C:\Windows » (et non « C:\\Windows »).
  for (const { node } of map.values()) node.path = node.path === "C:" ? ROOT : node.path.replace(/^C:\\\\/, ROOT);
  return map;
}

function nodes(): Map<NodeId, Built> {
  tree ??= build();
  return tree;
}

/** Deux niveaux sous le nœud, comme la tranche du backend. */
function slice(node: TreeNode, levels: number): TreeNode {
  return { ...node, children: levels === 0 ? [] : node.children.map((c) => slice(c, levels - 1)) };
}

export function mockDepthTree(node: NodeId | null): TreeSlice {
  const all = nodes();
  // Le profil d'un autre compte n'a pas d'enfants : il ne s'ouvre jamais.
  const id = node !== null && all.get(node)?.node.hasChildren ? node : 0;
  const chain: Crumb[] = [];
  for (let cur: NodeId | null = id; cur !== null && cur !== 0; cur = all.get(cur)?.parent ?? null) {
    const n = all.get(cur)?.node;
    if (n) chain.unshift({ label: n.name, id: cur });
  }
  const root = all.get(id)?.node ?? (all.get(0) as Built).node;
  return { root: slice(root, 2), breadcrumb: [{ label: "C:", id: 0 }, ...chain] };
}

function find(name: string, parent = 0): Built | undefined {
  return [...nodes().values()].find((b) => b.parent === parent && b.node.name === name);
}

function entry(b: Built | undefined, name?: string): DepthEntry[] {
  return b ? [{ name: name ?? b.node.name, path: b.node.path, size: b.node.size, node: b.node.id }] : [];
}

function childrenOf(name: string, skip: string[] = []): DepthEntry[] {
  const p = find(name);
  if (!p) return [];
  return p.node.children.filter((c) => !skip.includes(c.name)).map((c) => ({ name: c.name, path: c.path, size: c.size, node: c.id }));
}

const sum = (entries: DepthEntry[]) => entries.reduce((s, e) => s + e.size, 0);

export function mockDepthReport(scanId: number, finishedAt: number): DepthReport {
  const win = find("Windows");
  const sys32 = win && find("System32", win.node.id);
  const drivers = sys32 && find("DriverStore", sys32.node.id);
  const winSxs = win && find("WinSxS", win.node.id);
  const windowsEntries: DepthEntry[] = [
    ...entry(winSxs),
    ...(sys32 ? [{ ...entry(sys32)[0], size: sys32.node.size - (drivers?.node.size ?? 0) } as DepthEntry] : []),
    ...entry(drivers),
    { name: "", path: "C:\\Windows", size: (win?.node.size ?? 0) - (winSxs?.node.size ?? 0) - (sys32?.node.size ?? 0), node: win?.node.id ?? null },
  ];
  const apps = [...childrenOf("Program Files", ["WindowsApps"]), ...childrenOf("Program Files (x86)")].sort((a, b) => b.size - a.size);
  const store = [...childrenOf("Program Files").filter((e) => e.name === "WindowsApps"), ...entry(find("XboxGames"))];
  const files = ["hiberfil.sys", "pagefile.sys", "swapfile.sys"].flatMap((f) => entry(find(f)));
  const users = find("Users");
  const me = users && find("arthur", users.node.id);
  const others = users ? users.node.children.filter((c) => c.name !== "arthur").map((c) => ({ name: c.name, path: c.path, size: c.size, node: null })) : [];
  const roots = ["Riot Games", "Android"].flatMap((r) => entry(find(r)));
  const measured = nodes().get(0)?.node.size ?? 0;
  const items: DepthItem[] = [
    { id: "windows", size: win?.node.size ?? 0, files: win?.node.files ?? 0, entries: windowsEntries, link: "sinks" },
    { id: "programFiles", size: sum(apps), files: 0, entries: apps, link: "uninstall" },
    { id: "programData", size: find("ProgramData")?.node.size ?? 0, files: 0, entries: childrenOf("ProgramData"), link: "dev" },
    { id: "systemFiles", size: sum(files), files: files.length, entries: files, link: "sinks" },
    { id: "restore", size: find("System Volume Information")?.node.size ?? 0, files: 0, entries: [], link: "sinks" },
    { id: "profiles", size: sum(others), files: 0, entries: others, link: null },
    { id: "storeApps", size: sum(store), files: 0, entries: store, link: "games" },
    { id: "rootApps", size: sum(roots), files: 0, entries: roots, link: "games" },
    { id: "userProfile", size: me?.node.size ?? 0, files: me?.node.files ?? 0, entries: entry(me), link: "analysis" },
    { id: "other", size: find("$Recycle.Bin")?.node.size ?? 0, files: 0, entries: entry(find("$Recycle.Bin")), link: null },
    { id: "inaccessible", size: Math.max(0, gb(USED) - measured), files: 0, entries: [], link: null },
  ];
  items.sort((a, b) => b.size - a.size);
  return { scanId, volume: "C:", method: "walk", used: gb(USED), measured, accessDenied: 214, items, finishedAt };
}
