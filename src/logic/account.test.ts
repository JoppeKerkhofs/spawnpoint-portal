import { expect, test } from "vitest";
import { nameHelp } from "./account";

test("name help", () => {
  expect(nameHelp("JoppeK", "JoppeK", false)).toEqual({ text: "Letters, numbers and _ only, 3 to 16 characters. Takes effect the next time you start a game.", invalid: false, canSave: false });
  expect(nameHelp("Jo", "JoppeK", false)).toEqual({ text: "Use at least 3 characters.", invalid: true, canSave: false });
  expect(nameHelp("", "JoppeK", false).invalid).toBe(true);
  expect(nameHelp("Jo ppe", "JoppeK", false)).toMatchObject({ text: "Only letters, numbers and _ are allowed.", invalid: true, canSave: false });
  expect(nameHelp("Jöppe", "JoppeK", false).invalid).toBe(true);
  expect(nameHelp("a".repeat(17), "JoppeK", false).invalid).toBe(true);
  expect(nameHelp("Steve_01", "JoppeK", false).canSave).toBe(true);
  expect(nameHelp("Steve_01", "Steve_01", true).text).toBe("Saved. Your next game uses this name.");
  expect(nameHelp("Steve_02", "Steve_01", true).canSave).toBe(true);
});
