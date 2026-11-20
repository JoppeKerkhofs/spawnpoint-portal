import { expect, test } from "vitest";
import { coverRect, shownPicture } from "./picture";

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
