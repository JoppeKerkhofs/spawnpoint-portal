import { expect, test } from "vitest";
import { andList, clock, count, countdown, errorText, lastPlayedText, megabytes, stageText } from "./format";

test("stage names and numbers", () => {
  expect(stageText("assets")).toBe("Game assets");
  expect(stageText("loader libraries")).toBe("Mod loader libraries");
  expect(stageText("check")).toBe("Checking files");
  expect(stageText("shaders")).toBe("Shaders");
  expect(count(3891)).toBe("3,891");
  expect(megabytes(2_202_009)).toBe("2.1 MB");
  expect(megabytes(40_000)).toBe("0.1 MB");
  expect(countdown(272_000)).toBe("4:32");
  expect(countdown(-5)).toBe("0:00");
  expect(clock(new Date(2026, 10, 20, 14, 6))).toBe("14:06");
  expect(andList(["Joppe’s Spawnpoint", "Lien’s panel"])).toBe("Joppe’s Spawnpoint and Lien’s panel");
  expect(andList(["A", "B", "C"])).toBe("A, B and C");
});

test("last played", () => {
  const now = new Date(2026, 10, 20, 18, 0);
  const at = (d: Date) => Math.floor(d.getTime() / 1000);
  expect(lastPlayedText("Homestead", { started_at: at(new Date(2026, 10, 20, 14, 6)), seconds: 4320 }, now)).toBe("Last played Homestead today, for 1 h 12 min.");
  expect(lastPlayedText("Homestead", { started_at: at(new Date(2026, 10, 19, 9, 0)), seconds: 30 }, now)).toBe("Last played Homestead yesterday, for less than a minute.");
  expect(lastPlayedText("Homestead", { started_at: at(new Date(2026, 10, 3, 9, 0)), seconds: 7200 }, now)).toBe("Last played Homestead on 3 November, for 2 h.");
  expect(lastPlayedText("Homestead", { started_at: at(new Date(2026, 10, 3, 9, 0)), seconds: 600 }, now)).toBe("Last played Homestead on 3 November, for 10 min.");
  expect(lastPlayedText("Homestead", null, now)).toBeNull();
});

test("errors in words", () => {
  expect(errorText({ kind: "unreachable", message: "panel X: cannot be reached" })).toBe("The panel cannot be reached.");
  expect(errorText({ kind: "not_ready", message: "the server is still installing its pack" })).toBe("The server is still installing its pack.");
  expect(errorText({ kind: "network", message: "request failed." })).toBe("Request failed. Check your internet connection, then try again.");
});
