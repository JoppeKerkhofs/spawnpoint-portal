// The system the window runs on, for the few texts and styles that differ.
export type Os = "windows" | "linux" | "macos";

export const osOf = (userAgent: string): Os => (/Windows/.test(userAgent) ? "windows" : /Macintosh|Mac OS X/.test(userAgent) ? "macos" : "linux");

export const os = osOf(navigator.userAgent);

export const thisComputer = os === "windows" ? "this PC" : "this computer";
export const secretStore = os === "windows" ? "Windows Credential Manager" : os === "macos" ? "your Keychain" : "your keyring";
export const followSystem = os === "windows" ? "Follow Windows" : "Follow the system";
