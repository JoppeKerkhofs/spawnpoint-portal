import { describe, expect, test } from "vitest";
import type { Instance, PackRef, Server } from "../api";
import { initials, instanceTexture, serverPill, serverTexture } from "./visuals";

const mp = (catalogue: string) => ({ catalogue, project: "p", version: null, version_name: null });
const ref = (project: string): PackRef => ({ catalogue: "modrinth", project, version: "1", version_name: null });
const server = (o: Partial<Server>): Server => ({
  slug: "s", name: "S", hostname: "h", state: "running", game_version: null, modpack: null, loader: null, players: null, picture: null, ...o,
});
const inst = (o: Partial<Instance>): Instance => ({
  id: "i", name: "I", minecraft: "1.21", loader: null, source: { kind: "manual" }, managed: [],
  memory_mb: null, last_played: null, manual: [], skipped: [], picture: null, own_picture: false, ...o,
});

describe("visuals", () => {
  test("textures", () => {
    expect(serverTexture(server({ modpack: mp("modrinth") }))).toBe("machine");
    expect(serverTexture(server({ loader: { name: "fabric", version: "0.20.1" } }))).toBe("wood");
    expect(serverTexture(server({ loader: { name: "forge", version: null } }))).toBe("sand");
    expect(serverTexture(server({ loader: { name: "neoforge", version: null } }))).toBe("machine");
    expect(serverTexture(server({}))).toBe("grass");
    expect(instanceTexture(inst({ source: { kind: "manual" }, loader: null }))).toBe("stone");
    expect(instanceTexture(inst({ source: { kind: "pack", pack: ref("ftb") } }))).toBe("machine");
    expect(instanceTexture(inst({ source: { kind: "server", panel: "p", server: "s", pack: ref("x") } }))).toBe("machine");
    expect(instanceTexture(inst({ source: { kind: "server", panel: "p", server: "s", pack: null } }))).toBe("grass");
    expect(instanceTexture(inst({ loader: { kind: "fabric", version: null } }))).toBe("wood");
  });
  test("initials", () => {
    expect(initials("JoppeK")).toBe("JK");
    expect(initials("steve")).toBe("S");
    expect(initials("ABC")).toBe("AB");
    expect(initials("AbC De")).toBe("AC");
    expect(initials("")).toBe("");
  });
  test("pills", () => {
    expect(serverPill("running")).toEqual({ tone: "ok", text: "Running" });
    expect(serverPill("installing")).toEqual({ tone: "busy", text: "Installing" });
    expect(serverPill("offline")).toEqual({ tone: "idle", text: "Offline" });
    expect(serverPill("starting")).toEqual({ tone: "error", text: "Error" });
  });
});
