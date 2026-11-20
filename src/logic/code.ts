export const codeGroups = (code: string): string => code.match(/.{1,4}/g)?.join(" ") ?? "";

export const codeSpoken = (code: string): string => `Code: ${Array.from(code).join(" ")}`;
