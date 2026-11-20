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

Every push to `main` runs `.github/workflows/release.yml`. When the version in `package.json` has no tag `v<version>` yet, it runs the tests, builds and signs the installer, keeps it as a workflow artifact for 30 days, uploads it to the panel, and then creates the tag on the released commit. When the tag exists, it does nothing: a push that does not change the version releases nothing. A run that failed can be run again; if the panel already has the version (409), the run only adds the missing tag.

It needs two secrets, kept in a GitHub environment named `release` (Settings, Environments):

- `TAURI_SIGNING_PRIVATE_KEY`: the content of the update signing key, `C:\Users\joppe\.tauri\spawnpoint-portal.key` on the owner's PC. It has no password. Never commit it; the matching public key is in `src-tauri/tauri.conf.json`.
- `PANEL_RELEASE_TOKEN`: a panel API token with the ability `portal:release`.

Adding required reviewers to that environment makes every release wait for approval; without them a merge to `main` with a new version releases to every player by itself. Protecting `main` (Settings, Rules) limits who can start a release.

Addresses: the app checks `https://spawnpoint.joppek.be/portal/latest.json`, releases are uploaded with `PUT https://spawnpoint.joppek.be/api/v1/portal/releases/<version>`, and players download from `https://spawnpoint.joppek.be/portal/download`.

Windows shows an "unknown publisher" warning on the installer because it is not code-signed. The update itself is checked against the signing key.

## Test

    cargo test --workspace
    npm test
    npx vue-tsc --noEmit

## Build the command-line tool

    cargo build

## Where data lives

Instances, game files, Java runtimes and settings are under `%APPDATA%\SpawnpointPortal`. A panel's key is kept in the Windows Credential Manager, never in a file. The window and the command line share both.

## The window

- **Servers**: every linked panel's servers, with state, players and what they run. Join syncs the pack and starts the game.
- **Instances**: your instances, and a form to create a plain one. Instances Portal cannot read are listed with a button to show their folder.
- **An instance**: memory, the mods folder, adding and removing your own mods, files a pack's author does not allow launchers to download (to fetch by hand), and delete.
- **Panels**: link a panel by approving a code on its page, or remove one.
- **Account**: the player name and the appearance (standard light, dark or following Windows, or blocky).

The status bar shows the steps of a join and whether the game is running.

## Command line

The `portal` tool does everything the window does, and also installs packs without a server (`portal pack install`), which the window does not.

### Commands

    portal versions                    list the release versions, newest first
    portal files <version>             download the game files of a version
    portal java <component>            download a Java runtime, e.g. java-runtime-delta
    portal instance list               id, name, version, loader and source of each instance
    portal instance create <name> <version> [--loader <fabric|neoforge|forge>[:<version>]]
                                       the loader's newest version is stored when none is given
    portal instance delete <id>
    portal instance mods <id>          name, size, pack or own
    portal instance add-mod <id> <jar>
    portal instance remove-mod <id> <file name>
    portal pack install ftb <pack> <version> [--name <name>]
    portal pack install modrinth <version id> [--name <name>]
                                       make an instance from a pack; files to fetch by hand are listed
    portal pack update <id> <version>  install another version of the instance's pack over it
    portal launch <id> [--name <player>] [--server <host>] [--memory <MB>]
                                       start the game offline (name defaults to Player, memory to 4096),
                                       wait for it to exit
    portal panel add <address> [--allow-http]
                                       link a Spawnpoint panel: approve the code it prints at
                                       <address>/settings/api-tokens (the token goes in the
                                       Windows Credential Manager, never in a file)
    portal panel list                  id, name and address of each linked panel
    portal panel remove <id>           unlink a panel and forget its token
    portal servers                     <panel id>/<server>, name, state, version, loader or pack
    portal sync <panel id>/<server>    make the server's instance match the server
    portal join <panel id>/<server> [--name <player>]
                                       sync, then start the game on that server; the panel id
                                       may be left out when one panel is linked
