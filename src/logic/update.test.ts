import { beforeEach, expect, test, vi } from "vitest";

vi.mock("../api", () => ({ updateCheck: vi.fn(), updateInstall: vi.fn() }));

import { updateCheck, updateInstall } from "../api";
import { busy } from "./operation";
import { availableVersion, canInstall, checkForUpdate, installBlock, installUpdate, update } from "./update";

const err = { kind: "other", message: "no network" };
const mocked = <T extends (...a: never[]) => unknown>(f: T) => vi.mocked(f);

beforeEach(() => {
  update.value = { phase: "idle" };
  vi.resetAllMocks();
});

test("a check ends current, available or failed", async () => {
  mocked(updateCheck).mockResolvedValueOnce(null);
  await checkForUpdate(true);
  expect(update.value).toEqual({ phase: "current" });
  mocked(updateCheck).mockResolvedValueOnce("0.2.0");
  await checkForUpdate(true);
  expect(update.value).toEqual({ phase: "available", version: "0.2.0" });
  mocked(updateCheck).mockRejectedValueOnce(err);
  await checkForUpdate(true);
  expect(update.value).toEqual({ phase: "failed", error: err, version: null });
});

test("the check shows as checking while it runs", async () => {
  let done: (v: string | null) => void = () => {};
  mocked(updateCheck).mockReturnValueOnce(new Promise((r) => (done = r)));
  const run = checkForUpdate(true);
  expect(update.value).toEqual({ phase: "checking" });
  await checkForUpdate(true);
  expect(updateCheck).toHaveBeenCalledTimes(1);
  done(null);
  await run;
});

test("the check at start is silent when it fails and does not replace a manual result", async () => {
  mocked(updateCheck).mockRejectedValueOnce(err);
  await checkForUpdate(false);
  expect(update.value).toEqual({ phase: "idle" });
  update.value = { phase: "current" };
  await checkForUpdate(false);
  expect(updateCheck).toHaveBeenCalledTimes(1);
});

test("a failed install keeps the version so it can be tried again", async () => {
  update.value = { phase: "available", version: "0.2.0" };
  mocked(updateInstall).mockRejectedValueOnce({ kind: "busy", message: "Close your game before updating." });
  await installUpdate();
  expect(update.value).toEqual({ phase: "failed", error: { kind: "busy", message: "Close your game before updating." }, version: "0.2.0" });
  expect(availableVersion(update.value)).toBe("0.2.0");
  expect(canInstall(update.value, false, false)).toBe(true);
});

test("only a found version is installed", async () => {
  await installUpdate();
  expect(updateInstall).not.toHaveBeenCalled();
  update.value = { phase: "available", version: "0.2.0" };
  mocked(updateInstall).mockImplementationOnce(async () => expect(update.value).toEqual({ phase: "installing", version: "0.2.0" }));
  await installUpdate();
  expect(updateInstall).toHaveBeenCalledTimes(1);
});

test("install is blocked while a game runs or a task is busy, game first", () => {
  expect(installBlock(false, false)).toBeNull();
  expect(installBlock(true, false)).toBe("Wait for the current task to finish.");
  expect(installBlock(false, true)).toBe("Close your game before updating.");
  expect(installBlock(true, true)).toBe("Close your game before updating.");
  const found = { phase: "available", version: "0.2.0" } as const;
  expect(canInstall(found, false, false)).toBe(true);
  expect(canInstall(found, true, false)).toBe(false);
  expect(canInstall(found, false, true)).toBe(false);
  expect(canInstall({ phase: "installing", version: "0.2.0" }, false, false)).toBe(false);
  expect(canInstall({ phase: "current" }, false, false)).toBe(false);
});

test("the version shows in the sidebar and status bar only once one was found", () => {
  expect(availableVersion({ phase: "idle" })).toBeNull();
  expect(availableVersion({ phase: "failed", error: err, version: null })).toBeNull();
  expect(availableVersion({ phase: "installing", version: "0.2.0" })).toBe("0.2.0");
});

test("the window is busy while the update installs and free again if it fails", async () => {
  let fail: (e: unknown) => void = () => {};
  mocked(updateInstall).mockReturnValueOnce(new Promise((_, r) => (fail = r)));
  update.value = { phase: "available", version: "0.2.0" };
  const run = installUpdate();
  expect(busy.value).toBe(true);
  fail(err);
  await run;
  expect(busy.value).toBe(false);
});
