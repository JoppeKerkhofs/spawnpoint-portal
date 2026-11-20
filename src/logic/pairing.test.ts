import { expect, test } from "vitest";
import type { PanelServers } from "../api";
import { canGetCode, linkedStatus, looksLikeAddress, missingText, needsConsent, pollVerdict } from "./pairing";

const p = { id: "w", address: "https://spawnpoint.woutd.be", name: "spawnpoint.woutd.be" };
const entry = (n: number): PanelServers => ({ panel: p, error: null, servers: Array.from({ length: n }, (_, i) => ({ slug: `s${i}` }) as never) });

test("address and consent", () => {
  expect(looksLikeAddress("https://spawnpoint.woutd.be")).toBe(true);
  expect(looksLikeAddress("http://10.0.0.5.nip.io:8080/x")).toBe(true);
  for (const bad of ["https://", "spawnpoint.woutd.be", "ftp://x.be", "https://localhost"]) expect(looksLikeAddress(bad)).toBe(false);
  expect(canGetCode("http://10.0.0.5.nip.io", false)).toBe(false);
  expect(canGetCode("http://10.0.0.5.nip.io", true)).toBe(true);
  expect(canGetCode("https://spawnpoint.woutd.be", false)).toBe(true);
  expect(needsConsent("HTTP://x.be")).toBe(true);
  expect(needsConsent("https://x.be")).toBe(false);
});

test("when polling stops", () => {
  const linked = { panel: p, missing: [] };
  expect(pollVerdict(0, 1000, { linked: null }, false)).toBe("continue");
  expect(pollVerdict(0, 1000, null, false)).toBe("continue");
  expect(pollVerdict(0, 1000, { linked }, false)).toBe("linked");
  expect(pollVerdict(1000, 1000, { linked: null }, false)).toBe("expired");
  expect(pollVerdict(0, 1000, { error: { kind: "unreachable", message: "x" } }, false)).toBe("unreachable");
  expect(pollVerdict(0, 1000, { error: { kind: "other", message: "the pairing code expired or was already used" } }, false)).toBe("failed");
  expect(pollVerdict(1000, 1000, { error: { kind: "unreachable", message: "x" } }, false)).toBe("expired");
  expect(pollVerdict(0, 1000, { linked }, true)).toBe("stopped");
});

test("linked status", () => {
  expect(linkedStatus(undefined)).toBeNull();
  expect(linkedStatus(entry(5))).toEqual({ ok: true, text: "Connected · 5 servers" });
  expect(linkedStatus(entry(1))).toEqual({ ok: true, text: "Connected · 1 server" });
  expect(linkedStatus({ panel: p, servers: [], error: { kind: "unreachable", message: "x" } })).toEqual({ ok: false, text: "The panel cannot be reached." });
});

test("missing permissions", () => {
  expect(missingText(["servers.read", "servers.join"])).toBe("The key lacks these permissions: servers.read and servers.join. Grant them on the panel’s token page.");
});
