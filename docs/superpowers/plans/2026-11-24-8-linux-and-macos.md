# Linux, with macOS in mind

**Goal:** Portal runs on Linux (x86_64): the window, signing in, linking a panel, and starting the game. Every choice is made so that macOS is a later piece of the same shape, not a second rewrite.

**Where it stands:** Portal builds and runs on Windows only. Tauri, the sign-in and the panel client already work on all three systems. The Windows assumptions sit in a handful of places, listed per piece below.

## Status

Updated 24 November 2026. Change this table in the pull request that changes a piece.

| Piece | State | Pull request | Still to check |
|---|---|---|---|
| 1. The tests on Linux | Merged | #11 | |
| 2. Game files | Merged | #12 | |
| 3. Java | Merged | #13 | |
| 4. Starting the game | Open | #14 | Starting the game on Linux; the Forge and NeoForge installer steps on Linux |
| 5. The rest of the core | Open | | Signing in and linking a panel on Linux, and both still there after a restart; presence in a Discord on Linux |
| 6. The window | Not started | | |
| 7. The panel serves more than one installer | Not started | | |
| 8. Releasing | Not started | | |
| macOS | Not started | | Everything; see "macOS, later" |

Tried on real Linux so far (WSL, Ubuntu 24.04): the core's tests, downloading and running Java 21 and Java 8, downloading the game files of 1.21.1, the default data folder, and keeping a secret in GNOME Keyring (unlocked, locked and absent). Nothing has been tried on a Linux desktop or a Mac.

**Scope**

- Linux x86_64 only. Mojang publishes no natives and no Java for Linux on ARM.
- One package: an AppImage. It is the only Linux format Tauri's updater can replace, and it runs on every distribution. A `.deb` can follow if someone asks.
- macOS: prepared for, not built. The last section says what is left.

## How the code is shaped for three systems

One new module, `crates/core/src/platform.rs`, holds every fact that differs per system:

```rust
pub struct Platform {
    pub os: &'static str,              // Mojang's name: "windows", "linux", "osx"
    pub arch: &'static str,            // "x86_64", "arm64"
    pub java_runtime: &'static str,    // key in Mojang's runtime list: "windows-x64", "linux", "mac-os", "mac-os-arm64"
    pub java_binary: &'static str,     // "bin/javaw.exe", "bin/java", "jre.bundle/Contents/Home/bin/java"
    pub console_java: &'static str,    // "bin/java.exe", otherwise the same as java_binary
    pub classpath_separator: &'static str,
}

impl Platform {
    pub const WINDOWS: Platform = ...;
    pub const LINUX: Platform = ...;
    pub fn current() -> &'static Platform   // chosen by cfg
}
```

The functions that decide something per system take a `&Platform` instead of asking `cfg`. That is the point of the struct: the Linux and macOS rules are then tested on the Windows build machine with saved Mojang answers, long before anyone starts a game there. `cfg` is used only where the code itself cannot compile elsewhere (the named pipe, the registry).

## Pieces

Each piece is one pull request that leaves Windows working. Pieces 1 to 5 are core only and can be tried from the command line in WSL (this laptop has Ubuntu 24.04 with a display, so the game window can show).

### 1. The tests on Linux

- The checks workflow gets a second job on `ubuntu-latest` that runs `cargo test -p portal-core`.
- Make the core's tests pass there. Some build paths like `r"C:\g"` by hand and compare them as text; those become paths joined from parts, or are marked Windows-only when they are about Windows (the file that cannot be removed while open, in `mods.rs`).
- `platform.rs` arrives with its first use, in piece 2.

Check: both jobs green. Nothing behaves differently yet.

### 2. Game files

`version.rs` today: a rule's system must be `windows`, natives are looked up under `windows`, and `${arch}` becomes `64`.

- `allowed` and `library_files` take the platform. A rule's `os.name` is compared with `platform.os`, its `os.arch` with the platform's.
- Natives: the classifier is looked up under `platform.os`. Newer versions list natives as libraries of their own (`...:natives-linux`) behind a rule, which the first point already covers.
- The skip of libraries named `-arm64` and `-x86` stays for x86_64 and becomes "skip what is not this arch", so `natives-macos-arm64` can be picked later.

Fixtures: the version files already saved (1.21.1, 1.16.5, 1.12.2, 1.7.10) contain the Linux and macOS entries; the tests assert which files each platform gets.

Check: `portal files 1.21.1` in WSL downloads `natives-linux` jars and no Windows ones.

### 3. Java

`java.rs` today: key `windows-x64`, binary `bin/javaw.exe`, and only entries of type `file` are written.

- The runtime key and the binary come from the platform.
- Linux runtimes hold two things Windows ones do not: entries of type `link` (symbolic links, to create) and `executable: true` (to set the mode on). Both are ignored on Windows as today.
- Save a real `all.json` cut down to the keys used, and a Linux file list with a link and an executable in it. The saved `java-all.json` holds only `windows-x64` now.

Check: `portal java java-runtime-delta` in WSL, then run the printed binary with `-version`.

To verify before building: the exact key names and the macOS layout, against Mojang's live list. They are from memory here.

### 4. Starting the game

`launch.rs` and `loader/forge.rs` today: `;` between classpath entries (three places), `java.exe` for installer steps.

- The separator and the console Java come from the platform.
- Version files carry system-specific Java arguments behind rules (`-XstartOnFirstThread` on macOS); they pass through `allowed` with the platform, so nothing is added by hand for 1.13 and newer. For older versions, which have one line of arguments and no rules, macOS will need that flag added; note it in the code where the old arguments are built.
- Rules can also name `os.version`; check how it is handled now and that a Linux launch does not pick up Windows-only arguments.

Check, in WSL from the command line, signed in with `portal login`:

- vanilla 1.21.1 opens its window and reaches the title screen;
- Fabric and NeoForge on 1.21.1;
- Forge 1.20.1, 1.12.2 and 1.7.10, the installer steps being the likeliest place for surprises;
- `portal join` on a real server.

### 5. The rest of the core

- **Data folder** (`paths.rs`, today `%APPDATA%\SpawnpointPortal`): on Linux `$XDG_DATA_HOME/spawnpoint-portal`, else `~/.local/share/spawnpoint-portal`. On macOS later `~/Library/Application Support/SpawnpointPortal`. `SPAWNPOINT_PORTAL_HOME` keeps winning everywhere.
- **Secrets** (`secrets.rs`): the `keyring` crate already brings the Linux backend (the desktop's secret service: GNOME Keyring, KWallet) and the macOS Keychain. A desktop without one gets a plain sentence: "Portal needs a keyring to keep your sign-in; install GNOME Keyring or KWallet." No fallback to a file.
- **Device name** (`COMPUTERNAME`): the host name on Linux and macOS, and "this computer" instead of "this PC" when there is none.
- **Discord** (`discord.rs`, a named pipe): a socket file on Linux and macOS, looked for in `$XDG_RUNTIME_DIR` and the folders Flatpak and Snap put it in. Only the connect and the link's type are per system; framing and activity are shared.
- **Bedrock**: already Windows-only; the row does not show elsewhere.
- **Syncing on a disk that tells case apart** (`sync.rs`): paths are compared without case, which matches Windows. On Linux a pack file whose path only changes case (`mods/a.jar` to `Mods/A.jar`) would be written beside the old one, and the old one never removed. Compare by exact path there, and let the test that is Windows-only now cover both.

Check: sign in, link a panel, restart, and both are still there; presence shows in Discord for Linux.

### 6. The window

- The Ubuntu job installs Tauri's build packages (WebKitGTK 4.1 and friends) and runs the whole workspace's tests.
- Bundle targets move out of the shared config: `nsis` for Windows, `appimage` in `tauri.linux.conf.json`. Tauri merges the file for the system it builds on.
- The window draws its own title bar (`decorations: false`). Keep that on Linux and check it: moving, resizing from the edges, double-click to maximise. Some window managers give an undecorated window no resize edges; if that shows, Linux gets the system's title bar and ours is hidden there.
- The page gets the system's name once (`data-os` on `<html>`), for the few texts and styles that differ: "this PC", "Windows Credential Manager", the "Follow Windows" appearance choice.
- Go through `docs/superpowers/specs/2026-11-20-window-run-through.md` on Linux. WebKitGTK is a different engine than the one on Windows; fonts, focus rings and the blocky look are where differences show.

Check: the run-through, in WSL and on one real Linux desktop if a friend has one.

### 7. The panel serves more than one installer (spawnpoint repository)

Today a release is one file, `spawnpoint-portal-<version>-setup.exe`, and `latest.json` names one platform.

- A release is stored per version and platform. `PUT /api/v1/portal/releases/{version}` takes a `platform` (`windows-x86_64`, `linux-x86_64`, later `darwin-aarch64` and `darwin-x86_64`); without it, Windows, so the current workflow keeps working.
- `latest.json` lists every platform the newest version has. A system the newest version was not built for gets no update offer, which is the right outcome.
- The download page offers the visitor's system first and the others below. Marking a version not downloadable and removing it stay per version.
- File names come from the version and a fixed list of platforms, never from what was sent, as now.

Check: Pest, and upload a Linux file to a local panel and read `latest.json`.

### 8. Releasing

- The release job becomes one job per system, each building, signing with the updater key and uploading its file with its platform.
- The tag is made by a last job that runs only when all of them succeeded, so a half release is run again instead of being tagged.
- An AppImage installed by hand updates itself through the same feed.

Check: a release reaches the panel with two platforms, and a Linux install on the previous version offers and installs the update.

## macOS, later

Prepared by the pieces above: the platform struct, rules and natives by system and arch, the Java layout, the data folder, the keyring, the Discord socket, releases per platform. Left to do:

- `Platform::MACOS` for both chips, and the `-XstartOnFirstThread` flag for old versions.
- Minecraft before 1.19 has no natives for Apple chips. Either those versions run the Intel Java through Rosetta, or they are not offered on Apple chips. To decide then.
- The title bar: the system's own buttons on the left instead of ours.
- Signing and notarising with the friend's developer account, on the friend's runner. The certificate and the notarisation credentials go in as secrets of that runner's environment, not in the repository.
- Every check of pieces 4 and 6 again, on a real Mac.

## Settled on 24 November 2026

1. AppImage only.
2. Without a keyring there is no sign-in: Portal asks the player to install one first.
3. Piece 7 is panel version 1.9.5.
4. Testing on a real Linux desktop comes later. Until then the checks are the Linux job in the workflow, and WSL where it has been set up.

## Risks

- The Forge installer steps on Linux, mostly for 1.6.4 to 1.12.2. They are tried in piece 4 before anything is built on top.
- WebKitGTK showing the window differently. Found in piece 6 by the run-through, fixed in CSS.
- The game in WSL uses a translated graphics driver. A failure there is not proof of a failure on real Linux, and the other way round.
- The Java facts for Linux and macOS in piece 3 are from memory and are checked against Mojang's list first.
