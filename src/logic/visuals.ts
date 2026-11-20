import type { Instance, Server } from "../api";

export type Texture = "grass" | "wood" | "sand" | "machine" | "stone" | "wool" | "chest";
export type PillTone = "ok" | "busy" | "idle" | "error";

const byLoader: Record<string, Texture> = { fabric: "wood", forge: "sand", neoforge: "machine" };

export function serverTexture(s: Server): Texture {
  if (s.modpack) return "machine";
  return (s.loader && byLoader[s.loader.name]) || "grass";
}

export function instanceTexture(i: Instance): Texture {
  const pack = i.source.kind === "pack" || (i.source.kind === "server" && i.source.pack !== null);
  if (pack) return "machine";
  if (i.loader) return byLoader[i.loader.kind] ?? "grass";
  return i.source.kind === "manual" ? "stone" : "grass";
}

export function initials(name: string): string {
  const caps = name.match(/\p{Lu}/gu);
  return caps ? caps.slice(0, 2).join("") : (Array.from(name)[0] ?? "").toUpperCase();
}

export function serverPill(state: string): { tone: PillTone; text: string } {
  switch (state) {
    case "running":
      return { tone: "ok", text: "Running" };
    case "installing":
      return { tone: "busy", text: "Installing" };
    case "offline":
      return { tone: "idle", text: "Offline" };
    default:
      return { tone: "error", text: "Error" };
  }
}
