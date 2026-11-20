const rule = "Letters, numbers and _ only, 3 to 16 characters. Takes effect the next time you start a game.";

export function nameHelp(name: string, saved: string, justSaved: boolean): { text: string; invalid: boolean; canSave: boolean } {
  if (name.length < 3) return { text: "Use at least 3 characters.", invalid: true, canSave: false };
  if (!/^[A-Za-z0-9_]+$/.test(name) || name.length > 16) return { text: "Only letters, numbers and _ are allowed.", invalid: true, canSave: false };
  if (justSaved && name === saved) return { text: "Saved. Your next game uses this name.", invalid: false, canSave: false };
  return { text: rule, invalid: false, canSave: name !== saved };
}
