export interface CommandError {
  kind: string;
  message: string;
}

// Rejections from outside a Rust handler (permissions, unknown command, bad arguments) are plain strings.
export function normalizeError(e: unknown): CommandError {
  if (typeof e === "object" && e !== null && typeof (e as CommandError).kind === "string" && typeof (e as CommandError).message === "string") {
    return e as CommandError;
  }
  return { kind: "other", message: String(e) };
}
