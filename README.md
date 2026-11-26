# spawnpoint-portal

A Minecraft launcher for Windows. It links to your Spawnpoint panels, lists their servers, and joins one with a single click: it syncs the server's pack into an instance and starts the game on that server.

It has three parts: `portal-core` (a Rust library with all the launcher logic), `portal` (a command-line tool), and Spawnpoint Portal, the window (Tauri v2 with a Vue 3 and TypeScript frontend).

Every Minecraft release from 1.0 on is supported as vanilla; alpha and beta versions are not. Fabric and NeoForge start at the versions they exist for; Forge works from 1.6.1 on, except that a Forge for 1.6.x or 1.7.x that needs a `.pack.xz` library is refused with a message naming it. Forge on Minecraft 1.7.2 does not start on Java 8 without the legacyjavafixer mod in the instance's mods folder.

## Install

Download the installer from https://spawnpoint.joppek.be/portal/download and run it. It installs for the current user, no administrator rights needed. Windows shows an "unknown publisher" warning, because the installer is not code-signed. The app checks for newer versions from the Account page and, once at start, tells you in the sidebar and status bar when one exists; it never installs by itself.

## Run in development

    npm install
    npm run tauri dev

This needs Node.js, a Rust toolchain and the Tauri prerequisites for Windows.

## Build the window

    npm run tauri build -- --config src-tauri/tauri.local.json

This builds the installer `target\release\bundle\nsis\Spawnpoint Portal_<version>_x64-setup.exe`, which installs for the current user without administrator rights. The extra config turns off the update signature, which needs the release key; the installer is not code-signed either way. To build only the program, add `--no-bundle`.

The version is the `version` line in `package.json`; `tauri.conf.json` follows it.

## Releasing

1. Run `npm version <x.y.z> --no-git-tag-version` (it updates `package.json` and `package-lock.json` together; `tauri.conf.json` follows) and commit.
2. Get that commit onto `main`.

Every push to `main` runs `.github/workflows/release.yml`. When the version in `package.json` has no tag `v<version>` yet, it builds one installer per system, each in a job of its own: the Windows installer and the Linux AppImage. Each job runs the tests, builds and signs its installer, keeps it as a workflow artifact for 30 days and uploads it to the panel with its platform. A last job creates the tag on the released commit, and only when every system was uploaded. When the tag exists, the workflow does nothing: a push that does not change the version releases nothing. A run that failed can be run again; a system the panel already has the version for (409) is skipped, so the run adds what is missing and then the tag.

It needs two secrets, kept in a GitHub environment named `release` (Settings, Environments):

- `TAURI_SIGNING_PRIVATE_KEY`: the content of the update signing key, `C:\Users\joppe\.tauri\spawnpoint-portal.key` on the owner's PC. It has no password. Never commit it; the matching public key is in `src-tauri/tauri.conf.json`.
- `PANEL_RELEASE_TOKEN`: a panel API token with the ability `portal:release`.

macOS is built only when the repository variable `MACOS_RUNNER` (Settings, Secrets and variables, Actions, Variables) names a runner with an Apple chip: `macos-latest` for GitHub's own, which costs ten times the minutes on a private repository, or the label of someone's Mac. The same variable turns on the macOS job of the checks. The panel must be 1.9.6 or newer first, or it refuses the upload. The app is signed and notarised when the `release` environment also holds `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, and on GitHub's runners `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`; `docs/macos.md` says where each comes from. Without them the app is signed for the build machine only, and other Macs refuse it until their owner allows it.

Adding required reviewers to that environment makes every release wait for approval; without them a merge to `main` with a new version releases to every player by itself. Protecting `main` (Settings, Rules) limits who can start a release.

Addresses: the app checks `https://spawnpoint.joppek.be/portal/latest.json`, releases are uploaded with `PUT https://spawnpoint.joppek.be/api/v1/portal/releases/<version>?platform=<windows-x86_64, linux-x86_64 or darwin-aarch64>`, and players download from `https://spawnpoint.joppek.be/portal/download`.

Windows shows an "unknown publisher" warning on the installer because it is not code-signed. The update itself is checked against the signing key.

## Test

    cargo test --workspace
    npm test
    npx vue-tsc --noEmit

The core's tests also run on Linux in the checks. To try the Linux side on a Windows machine, see `docs/linux-in-wsl.md`.

## Build the command-line tool

    cargo build

## Where data lives

Instances, game files, Java runtimes and settings are under `%APPDATA%\SpawnpointPortal` (on Linux `~/.local/share/spawnpoint-portal`, or `$XDG_DATA_HOME/spawnpoint-portal`; on macOS `~/Library/Application Support/SpawnpointPortal`). A panel's key is kept in the Windows Credential Manager, never in a file. The window and the command line share both.

## The window

- **Servers**: every linked panel's servers, with state, players and what they run. Join syncs the pack and starts the game.
- **Instances**: your instances, a form to create a plain one, and **Install a pack**: search Modrinth's packs, pick a version and install it as an instance, without a server. Instances Portal cannot read are listed with a button to show their folder.
- **An instance**: memory, your own Java arguments (and, read-only, the ones its server sets for every player), another version of its pack or a new check of the pack's files (for a Modrinth pack installed without a server), renaming it in its title (the pencil), duplicating it (a copy named after it, with its mods, configs and worlds), another Minecraft version or mod loader for an instance you made yourself, the mods folder, adding and removing your own mods, and the same for resource packs and shaders (browsed on Modrinth, or a zip file of your own), files a pack's author does not allow launchers to download (to fetch by hand), the log of its last start (the newest crash report, what the game printed and `latest.log`, with any sign-in token left out), and delete.
- **Panels**: link a panel by approving a code on its page, or remove one.
- **Account**: signing in with Microsoft, which starting a game needs (one account; only its sign-in is kept, in the Windows Credential Manager), and the appearance (standard light, dark or following Windows, or blocky).

The status bar shows the steps of a join and whether the game is running, and offers the log when a game stops without being closed.

## Command line

The `portal` tool does everything the window does, except searching for packs: `portal pack install` takes a pack's ids. It also installs FTB packs without a server, which the window does not.

### Commands

    portal versions                    list the release versions, newest first
    portal files <version>             download the game files of a version
    portal java <component>            download a Java runtime, e.g. java-runtime-delta
    portal instance list               id, name, version, loader and source of each instance
    portal instance create <name> <version> [--loader <fabric|neoforge|forge>[:<version>]]
                                       the loader's newest version is stored when none is given
    portal instance delete <id>
    portal instance rename <id> <name> the id, and so the folder, stays
    portal instance duplicate <id> <name>
                                       a copy of the whole instance, worlds included
    portal instance set-version <id> <version> [--loader <fabric|neoforge|forge>[:<version>]]
                                       for an instance you made yourself; its mods are left as they are
    portal instance java-args <id> [<arguments>]
                                       your own Java arguments, as one quoted text; none clears them
    portal instance mods <id> [--kind <resourcepack|shader>]
                                       name, size, pack or own; with --kind the resource packs or shaders
    portal instance add-mod <id> <file> [--kind <resourcepack|shader>]
    portal instance remove-mod <id> <file name> [--kind <resourcepack|shader>]
    portal pack install ftb <pack> <version> [--name <name>]
    portal pack install modrinth <version id> [--name <name>]
                                       make an instance from a pack; files to fetch by hand are listed
    portal pack update <id> <version>  install another version of the instance's pack over it
    portal launch <id> [--server <host>] [--memory <MB>]
                                       start the game (memory defaults to 4096), wait for it to exit
    portal login                       sign in with Microsoft: open the page it prints and enter the code
    portal logout                      forget the Microsoft sign-in
    portal whoami                      the signed-in player; join and launch need a sign-in
    portal panel add <address> [--allow-http]
                                       link a Spawnpoint panel: approve the code it prints at
                                       <address>/settings/api-tokens (the token goes in the
                                       Windows Credential Manager, never in a file)
    portal panel list                  id, name and address of each linked panel
    portal panel remove <id>           unlink a panel and forget its token
    portal servers                     <panel id>/<server>, name, state, version, loader or pack
    portal sync <panel id>/<server>    make the server's instance match the server
    portal join <panel id>/<server>
                                       sync, then start the game on that server; the panel id
                                       may be left out when one panel is linked
