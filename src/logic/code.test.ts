import { expect, test } from "vitest";
import { codeGroups, codeSpoken } from "./code";

test("pairing code", () => {
  expect(codeGroups("7ABR547C")).toBe("7ABR 547C");
  expect(codeGroups("ABC")).toBe("ABC");
  expect(codeSpoken("7ABR547C")).toBe("Code: 7 A B R 5 4 7 C");
});
