import type { CommandError, LastPlayed } from "../api";

const stages: Record<string, string> = {
  check: "Checking files",
  java: "Java",
  libraries: "Libraries",
  assets: "Game assets",
  "loader libraries": "Mod loader libraries",
  loader: "Mod loader",
  mods: "Mods",
};

export const upper = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

export const stageText = (stage: string) => stages[stage] ?? upper(stage);

export const minecraftText = (v: string | null): string => (v ? `Minecraft ${v}` : "—");

export const memoryLabel = (mb: number): string => `${Number((mb / 1024).toFixed(1))} GB`;

export const count = (n: number) => n.toLocaleString("en-US");

export function megabytes(bytes: number): string {
  const mb = bytes / 1048576;
  return `${bytes > 0 && mb < 0.05 ? "0.1" : mb.toFixed(1)} MB`;
}

const pad = (n: number) => String(n).padStart(2, "0");

export const clock = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;

export function countdown(ms: number): string {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(s / 60)}:${pad(s % 60)}`;
}

export function andList(names: string[]): string {
  return names.length < 2 ? (names[0] ?? "") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

export function sentence(text: string): string {
  const t = upper(text.trim());
  return /[.!?]$/.test(t) ? t : `${t}.`;
}

export function errorText(e: CommandError): string {
  if (e.kind === "unreachable") return "The panel cannot be reached.";
  if (e.kind === "network") return `${sentence(e.message)} Check your internet connection, then try again.`;
  return sentence(e.message);
}

export function lastPlayedText(name: string, lp: LastPlayed | null, now: Date): string | null {
  if (!lp) return null;
  const at = new Date(lp.started_at * 1000);
  const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
  const day = same(at, now)
    ? "today"
    : same(at, new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1))
      ? "yesterday"
      : `on ${at.getDate()} ${months[at.getMonth()]}`;
  const h = Math.floor(lp.seconds / 3600);
  const m = Math.floor((lp.seconds % 3600) / 60);
  const length = lp.seconds < 60 ? "less than a minute" : h === 0 ? `${m} min` : m === 0 ? `${h} h` : `${h} h ${m} min`;
  return `Last played ${name} ${day}, for ${length}.`;
}
