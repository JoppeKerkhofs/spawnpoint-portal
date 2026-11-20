import { expect, test } from "vitest";
import { coverRect, headUrl, shownPicture } from "./picture";

test("a cover fit crops the longer side, centred", () => {
  expect(coverRect(200, 100)).toEqual({ sx: 50, sy: 0, size: 100 });
  expect(coverRect(100, 300)).toEqual({ sx: 0, sy: 100, size: 100 });
  expect(coverRect(64, 64)).toEqual({ sx: 0, sy: 0, size: 64 });
});

test("a picture shows unless there is none or it failed to load", () => {
  expect(shownPicture("data:image/png;base64,AA", false)).toBe("data:image/png;base64,AA");
  expect(shownPicture(null, false)).toBeNull();
  expect(shownPicture(undefined, false)).toBeNull();
  expect(shownPicture("", false)).toBeNull();
  expect(shownPicture("https://cdn.modrinth.com/a.png", true)).toBeNull();
});

test("a head address is built only for a Java player name", () => {
  expect(headUrl("Notch_1")).toBe("https://mc-heads.net/avatar/Notch_1/48");
  expect(headUrl("a".repeat(16))).toContain("/avatar/aaaa");
  for (const bad of ["", "a".repeat(17), ".Bedrock Guy", "a b", "a/../b", "a?x=1", "é"]) expect(headUrl(bad)).toBeNull();
});
