import { expect, test, vi } from "vitest";
import type { Progress } from "../api";

let emit: (p: Progress) => void = () => {};
vi.mock("../api", () => ({
  onProgress: vi.fn(async (cb: (p: Progress) => void) => {
    emit = cb;
    return () => {};
  }),
  running: vi.fn(),
  defaults: vi.fn(),
  instanceGet: vi.fn(),
}));

import { defaults, instanceGet, running, type Instance } from "../api";
import { busy, closeFailure, gameExited, liveText, loadPlaying, markPlaying, onProgress, op, openInstanceId, openLog, playing, showLog, stopped, trailing, view, withProgress } from "./operation";

const inst = (id: string, extra: Partial<Instance> = {}): Instance => ({
  id, name: id.toUpperCase(), minecraft: "1.20.1", loader: null, source: { kind: "server", panel: "p", server: id, pack: null },
  managed: [], memory_mb: null, last_played: { started_at: 5, seconds: 60 }, played_seconds: 0, manual: [], skipped: [], picture: null, own_picture: false, java_args: "", server_java_args: [], refused_java_args: [], ...extra,
});

const p = [{ instance: "homestead", name: "Homestead", since: new Date(2026, 10, 20, 14, 6), host: "homestead.mc.joppek.be", memoryMb: 6144, serverKey: "p/homestead" }];

test("progress events move the bar, and only while preparing", () => {
  const prep = onProgress({ phase: "preparing", name: "Homestead", step: 0, steps: 0, stage: "", done: 0, total: 0 }, { step: 6, steps: 6, stage: "mods", done: 142, total: 366 });
  expect(prep).toMatchObject({ phase: "preparing", step: 6, stage: "mods", done: 142 });
  expect(onProgress(prep, { step: 6, steps: 6, stage: "starting", done: 0, total: 0 })).toEqual({ phase: "starting", name: "Homestead" });
  expect(onProgress({ phase: "idle" }, { step: 1, steps: 1, stage: "mods", done: 1, total: 1 })).toEqual({ phase: "idle" });
});

test("a failure stays until Close, then the next operation runs", async () => {
  await withProgress("Skyblock test", () => Promise.reject({ kind: "other", message: "x" }));
  expect(op.value.phase).toBe("failed");
  expect(busy.value).toBe(false);
  closeFailure();
  expect(await withProgress("Main", async () => 1)).toBe(1);
  expect(op.value.phase).toBe("idle");
});

test("one operation at a time", async () => {
  let release!: () => void;
  const first = withProgress("A", () => new Promise<void>((r) => (release = r)));
  await Promise.resolve();
  expect(busy.value).toBe(true);
  expect(await withProgress("B", async () => 2)).toBeUndefined();
  release();
  await first;
});

test("events after the operation resolved are ignored", async () => {
  await withProgress("A", async () => 1);
  emit({ step: 1, steps: 2, stage: "mods", done: 1, total: 2 });
  expect(op.value).toEqual({ phase: "idle" });
});

test("trailing text per view", () => {
  const ctx = { playerName: "JoppeK", lastPlayed: null };
  expect(trailing("servers", { phase: "idle" }, p, ctx)).toEqual({ lead: "Playing Homestead", muted: "since 14:06 · connected to homestead.mc.joppek.be" });
  expect(trailing("instance", { phase: "idle" }, p, ctx).muted).toBe("with 6 GB of memory");
  expect(trailing("account", { phase: "idle" }, p, ctx).muted).toBe("since 14:06 · as JoppeK");
  expect(trailing("servers", { phase: "idle" }, [{ ...p[0], host: null }], ctx).muted).toBe("since 14:06");
  expect(trailing("servers", { phase: "idle" }, [], ctx)).toEqual({ lead: "Ready", muted: "Join a server to start playing." });
  expect(trailing("instance", { phase: "idle" }, [], { ...ctx, lastPlayed: "Last played Homestead today, for 1 h 12 min." }).muted).toBe("Last played Homestead today, for 1 h 12 min.");
});

test("trailing text while working or failed", () => {
  const ctx = { playerName: "JoppeK", lastPlayed: null };
  const prep = { phase: "preparing" as const, name: "Homestead", step: 2, steps: 6, stage: "mods", done: 1, total: 9 };
  expect(trailing("servers", { ...prep, step: 0 }, p, ctx)).toEqual({ lead: "Preparing Homestead", muted: "" });
  expect(trailing("servers", prep, p, ctx).muted).toBe("Step 2 of 6: Mods");
  expect(trailing("servers", { phase: "starting", name: "Homestead" }, p, ctx).lead).toBe("Starting Minecraft for Homestead…");
  expect(trailing("servers", { phase: "failed", name: "Skyblock test", error: { kind: "other", message: "no build" } }, p, ctx)).toEqual({ lead: "Couldn’t prepare Skyblock test.", muted: "No build." });
});

test("the screen reader hears states and stages, not counts", () => {
  const prep = { phase: "preparing" as const, name: "Homestead", step: 2, steps: 6, stage: "mods", done: 1, total: 9 };
  expect(liveText({ ...prep, step: 0 }, [])).toBe("Preparing Homestead");
  expect(liveText(prep, [])).toBe(liveText({ ...prep, done: 5 }, []));
  expect(liveText({ phase: "idle" }, p)).toBe("Playing Homestead");
  expect(liveText({ phase: "idle" }, [])).toBe("");
});

test("a finished game leaves the list", () => {
  playing.value = [...p];
  gameExited("homestead", 0);
  expect(playing.value).toEqual([]);
  expect(stopped.value).toBeNull();
});

test("a game that did not close normally is named until its log is opened", () => {
  playing.value = [...p];
  gameExited("homestead", 1);
  expect(stopped.value).toEqual({ instance: "homestead", name: "Homestead" });
  const ctx = { playerName: "", lastPlayed: null, stopped: stopped.value!.name };
  expect(trailing("servers", { phase: "idle" }, [], ctx).lead).toBe("Homestead stopped unexpectedly.");
  expect(trailing("servers", { phase: "idle" }, p, ctx).lead).toBe("Playing Homestead");
  showLog("homestead");
  expect([stopped.value, openLog.value, openInstanceId.value, view.value]).toEqual([null, "homestead", "homestead", "instance"]);
});

test("progress events update op while an operation runs, with the key kept", async () => {
  let seen: unknown;
  await withProgress("Homestead", async () => {
    emit({ step: 2, steps: 6, stage: "mods", done: 3, total: 9 });
    seen = op.value;
    emit({ step: 6, steps: 6, stage: "starting", done: 0, total: 0 });
  }, "p/homestead");
  expect(seen).toEqual({ phase: "preparing", key: "p/homestead", name: "Homestead", step: 2, steps: 6, stage: "mods", done: 3, total: 9 });
});

test("op.key names the row being prepared", async () => {
  let release!: () => void;
  const first = withProgress("A", () => new Promise<void>((r) => (release = r)), "p/a");
  await Promise.resolve();
  expect(op.value).toMatchObject({ phase: "preparing", key: "p/a" });
  release();
  await first;
  await withProgress("B", async () => 1);
  expect(op.value).toEqual({ phase: "idle" });
});

test("a failure while a game is playing returns to playing after Close", async () => {
  const ctx = { playerName: "JoppeK", lastPlayed: null };
  playing.value = [...p];
  await withProgress("Skyblock test", () => Promise.reject({ kind: "other", message: "no build" }), "p/sky");
  expect(trailing("servers", op.value, playing.value, ctx).lead).toBe("Couldn\u2019t prepare Skyblock test.");
  expect(liveText(op.value, playing.value)).toBe("");
  closeFailure();
  expect(trailing("servers", op.value, playing.value, ctx).lead).toBe("Playing Homestead");
  playing.value = [];
});

test("the first new operation clears a failure", async () => {
  await withProgress("A", () => Promise.reject({ kind: "other", message: "x" }));
  let during: unknown;
  await withProgress("B", async () => {
    during = op.value.phase;
  });
  expect(during).toBe("preparing");
  expect(op.value).toEqual({ phase: "idle" });
});

test("loadPlaying takes the start time from the running list", async () => {
  vi.mocked(running).mockResolvedValue([{ instance: "homestead", started_at: 1_700_000_000 }]);
  vi.mocked(defaults).mockResolvedValue({ memory_choices_mb: [4096], default_memory_mb: 4096 });
  vi.mocked(instanceGet).mockResolvedValue(inst("homestead", { memory_mb: 6144 }));
  await loadPlaying();
  expect(playing.value).toEqual([{ instance: "homestead", name: "HOMESTEAD", since: new Date(1_700_000_000_000), host: null, memoryMb: 6144, serverKey: "p/homestead" }]);
  playing.value = [];
});

test("markPlaying adds an instance once", () => {
  playing.value = [];
  markPlaying(inst("atm10"), "atm10.example.org");
  markPlaying(inst("atm10"), "atm10.example.org");
  expect(playing.value).toHaveLength(1);
  expect(playing.value[0]).toMatchObject({ instance: "atm10", host: "atm10.example.org", memoryMb: 4096, serverKey: "p/atm10" });
  expect(playing.value[0].since).toBeInstanceOf(Date);
  playing.value = [];
});
