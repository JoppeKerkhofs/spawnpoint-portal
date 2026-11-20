import type { Instance, InstalledMod, Loader, ModFile, ModInstall, Panel } from "../api";
import { count, minecraftText } from "./format";
import { loaderText, packText } from "./instances";
import { loaderName } from "./servers";

export type ModFilter = "all" | "pack" | "own";

// Own mods first, then pack mods, each in the core's order (the sort is stable).
export const orderMods = (mods: ModFile[]): ModFile[] => [...mods].sort((a, b) => Number(a.managed) - Number(b.managed));

export function filterMods(mods: ModFile[], f: ModFilter, q: string): ModFile[] {
  const needle = q.trim().toLowerCase();
  return mods.filter((m) => (f === "all" || (f === "pack") === m.managed) && m.name.toLowerCase().includes(needle));
}

export function modCounts(mods: ModFile[]): { all: number; pack: number; own: number } {
  const pack = mods.filter((m) => m.managed).length;
  return { all: mods.length, pack, own: mods.length - pack };
}

export const modsText = (n: number): string => `${n} mod${n === 1 ? "" : "s"}`;

export const noModsText = (q: string): string => (q.trim() ? `No mod matches “${q}”.` : "No mods here.");

export function instanceSummary(i: Instance, panels: Panel[]): string {
  const s = i.source;
  const pack = s.kind === "pack" ? s.pack : s.kind === "server" ? s.pack : null;
  return [
    minecraftText(i.minecraft),
    i.loader && loaderText(i.loader),
    pack && packText(pack),
    s.kind === "server" && `from ${i.name} on ${panels.find((p) => p.id === s.panel)?.name ?? s.panel}`,
  ]
    .filter(Boolean)
    .join(" · ");
}

export const handHeading = (n: number): string => `${n} file${n === 1 ? "" : "s"} to download by hand`;

export const skippedName = (path: string): string => path.replace(/^mods\//, "");

// Only an https address is shown as a link; the backend refuses anything else as well.
export const isHttps = (url: string): boolean => /^https:\/\//i.test(url);

export function deleteBody(i: Instance, mods: ModFile[] | null): string {
  let what = "its mods";
  if (mods) {
    const { all, own } = modCounts(mods);
    what = `${modsText(all)}${own > 0 ? ` (including your ${own} own)` : ""}`;
  }
  const again = i.source.kind === "server" ? ` You can always join the ${i.name} server again to get a fresh copy.` : "";
  return `This removes the whole instance folder: ${what}, its configs, and its saved worlds. It can’t be undone.${again}`;
}

// The core's choices, plus a stored value outside them so the select never shows a wrong one.
export const memoryOptions = (choices: number[], current: number): number[] =>
  [...new Set([...choices, current])].sort((a, b) => a - b);

export type ResultControl = "install" | "pack" | "installed" | "update";

export function resultControl(project: string, installed: InstalledMod[] | null): ResultControl {
  const mine = (installed ?? []).filter((m) => m.project === project);
  if (mine.some((m) => m.managed)) return "pack";
  if (mine.length === 0) return "install";
  return mine.some((m) => m.update) ? "update" : "installed";
}

export function updatableMod(m: ModFile, known: InstalledMod[] | null): InstalledMod | undefined {
  const k = m.managed ? undefined : known?.find((x) => x.file === m.name);
  return k?.update && k.project ? k : undefined;
}

export function installedSentence(o: ModInstall): string {
  const needed = o.installed.length - 1;
  return [
    o.installed.length > 0 && `Installed ${o.installed[0]}${needed > 0 ? ` and ${modsText(needed)} it needs` : ""}.`,
    ...o.failed.map((f) => (f.reason.startsWith("The old file") ? f.reason : `${f.name} was not installed: ${f.reason}.`)),
  ]
    .filter(Boolean)
    .join(" ");
}

export const noResultsText = (l: Loader, minecraft: string): string => `No mods found for ${loaderName(l.kind)} on Minecraft ${minecraft}.`;

export const downloadsText = (n: number): string => `${count(n)} download${n === 1 ? "" : "s"}`;

export const browseNote = (i: Instance): string | null => (i.loader ? null : "Mods need a mod loader, and this instance has none.");
