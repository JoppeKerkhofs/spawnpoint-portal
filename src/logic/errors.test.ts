import { describe, expect, it } from "vitest";
import { normalizeError } from "./errors";

describe("normalizeError", () => {
  it("keeps a command error as it is", () => {
    const e = { kind: "busy", message: "this instance is running" };
    expect(normalizeError(e)).toEqual(e);
  });

  it("turns a string rejection into an error with a message", () => {
    expect(normalizeError("command nope not found")).toEqual({ kind: "other", message: "command nope not found" });
  });

  it("turns anything else into an error with a message", () => {
    expect(normalizeError(new Error("boom"))).toEqual({ kind: "other", message: "Error: boom" });
    expect(normalizeError(null)).toEqual({ kind: "other", message: "null" });
    expect(normalizeError({ kind: 1 })).toEqual({ kind: "other", message: "[object Object]" });
  });
});
