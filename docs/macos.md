# Building, trying and signing Portal on a Mac

Portal is written on a Windows machine, which cannot build or start a Mac program. Everything about macOS that needs a real Mac is done by you: building it, trying it, and at the end signing it. This page says what that takes. The work itself is laid out in `docs/superpowers/plans/2026-11-24-8-linux-and-macos.md`, pieces 9 to 12.

It is for a Mac with an Apple chip (M1 or newer). There is no build for Intel Macs.

## Where it stands

Nothing for macOS is merged yet. Each piece comes as a branch you are asked to try:

| Piece | What you do |
|---|---|
| 9. The core | Run the tests, start the game from the command line |
| 10. The window | Build the app, go through the checks below |
| 11. The panel | Nothing |
| 12. Releasing | Sign and notarise; maybe lend the Mac as a build machine |

Minecraft 1.19 and newer runs. Older versions have no Java and no game libraries for Apple chips from Mojang, so Portal refuses them with a sentence saying so.

## Set up, once

Apple's build tools (a window asks to install them):

```bash
xcode-select --install
```

Rust, for this user (accept the default):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

Node and the GitHub tool, with [Homebrew](https://brew.sh):

```bash
brew install node gh
```

The code. The repository is private, so sign in with the GitHub account that was given access:

```bash
gh auth login
mkdir -p ~/code && cd ~/code
gh repo clone JoppeKerkhofs/spawnpoint-portal
cd spawnpoint-portal
npm ci
```

## Check the setup

```bash
cargo --version && node --version
uname -m                      # arm64
cargo test -p portal-core
```

## Try a branch from the command line

```bash
cd ~/code/spawnpoint-portal
git fetch origin && git checkout <branch> && git pull
cargo test --workspace
```

Give a try a folder of its own, so it leaves nothing else behind. Without this Portal keeps its files in `~/Library/Application Support/SpawnpointPortal`.

```bash
export SPAWNPOINT_PORTAL_HOME=~/portal-try

cargo run -p portal-core -- java java-runtime-delta   # prints the Java it downloaded
cargo run -p portal-core -- files 1.21.1              # the game files of a version
```

Run the printed Java with `-version` to see that it starts.

Sign in and start the game:

```bash
cargo run -p portal-core -- login          # enter the code it prints at microsoft.com/link
cargo run -p portal-core -- instance create Vanilla 1.21.1
cargo run -p portal-core -- launch vanilla
```

The sign-in is kept in your Keychain. macOS asks whether `portal` may read it; "Always Allow" lasts until the next build, because a rebuilt program counts as a new one. Run `logout` when done. `~/portal-try` holds a few hundred megabytes and can be deleted at any time.

## Build and open the window

To work on it, with the page reloading as files change:

```bash
npm run tauri dev
```

To build the app as a player gets it:

```bash
npm run tauri build -- --config src-tauri/tauri.local.json
open "target/release/bundle/macos/Spawnpoint Portal.app"
```

The local config leaves out the update files, which need the release key that only Joppe has. The build also leaves a `.dmg` in `target/release/bundle/dmg`.

A build made this way is signed for your Mac only. Copied to another Mac it is refused: its owner has to open System Settings, Privacy & Security, and choose "Open Anyway". That goes away with the signing below.

## What to check

From the command line (piece 9):

- The tests pass.
- Vanilla, Fabric and NeoForge on 1.21.1 and Forge on 1.20.1 reach the title screen. Create them with `instance create <name> <version> --loader <fabric|neoforge|forge>`.
- An instance on 1.18.2 is refused with a sentence about Apple chips, not with an error about Java.
- `portal join` on a real server, once a panel is linked with `portal panel add <address>`.
- The game's window: does it open at a sensible size, does full screen work, is there sound.

In the window (piece 10):

- The three coloured buttons sit top left and work; the window moves by its top bar, resizes from every edge, and a double-click on the bar zooms it.
- Nothing of the page hides under those buttons.
- Sign in with Microsoft, link a panel, quit and open again: both are still there.
- Join a server from the Servers page. Create an instance, add a mod, delete it.
- The looks: light, dark, following the system, and blocky. Fonts, focus rings and the blocky borders are where Safari's engine differs from the one on Windows.
- With Discord open: your status shows the game while it runs.
- The full list is `docs/superpowers/specs/2026-11-20-window-run-through.md`.

What helps when something is off: the macOS version (`sw_vers`), the chip, what you did, what the terminal printed, a screenshot for anything about looks, and for a game that does not start the file `instances/<id>/game/logs/latest.log` under the Portal folder.

## Signing and notarising (piece 12, last)

Until this is done every player on a Mac gets the "Open Anyway" detour, and the Keychain asks again after each update. It needs a paid Apple Developer membership, which is yours; none of its secrets go into the repository or to anyone else.

What is below follows Tauri's guide for version 2, https://v2.tauri.app/distribute/sign/macos/. Read it beside this page: Apple changes these steps now and then.

**1. The certificate.** In your developer account create a certificate of the kind "Developer ID Application" (not "Apple Development", not "Mac App Store") and install it in your Keychain; Xcode does both under Settings, Accounts, Manage Certificates. Only the account's holder can create this kind. Its full name is what signs:

```bash
security find-identity -v -p codesigning
# 1) ABC123... "Developer ID Application: Your Name (TEAMID1234)"
```

**2. A password for notarising.** At https://account.apple.com, under Sign-In and Security, make an app-specific password. It is not your Apple password and can be withdrawn there at any time.

**3. A signed and notarised build.** Tauri signs, sends the app to Apple and staples the answer onto it when these are set in the terminal that builds:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID1234)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="the app-specific password"
export APPLE_TEAM_ID="TEAMID1234"

npm run tauri build -- --config src-tauri/tauri.local.json
```

Apple usually answers within minutes; the build waits for it.

**4. Check it.**

```bash
app="target/release/bundle/macos/Spawnpoint Portal.app"
codesign --verify --deep --strict --verbose=2 "$app"
spctl --assess --type execute -vv "$app"      # "accepted", source "Notarized Developer ID"
xcrun stapler validate "$app"
```

The check that counts: put the `.dmg` somewhere on the internet, download it with a browser on a Mac that never built Portal, and open it. It must start with no more than the question whether to open a program from the internet.

Then try what signing can break: sign in (the Keychain), start a game (Portal starts Mojang's Java, a program of its own), and Discord.

**5. For the release workflow.** A release is built by GitHub's workflow, not by hand, so the same four values plus the certificate end up as secrets of the repository's `release` environment. Joppe adds the workflow's side; you add the secrets yourself in GitHub (Settings, Environments, release), where nobody can read them back:

- `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` as above.
- `APPLE_CERTIFICATE`: the certificate with its private key, exported from Keychain Access as a `.p12` file with a password, then `base64 -i certificate.p12 | pbcopy`.
- `APPLE_CERTIFICATE_PASSWORD`: that password.

Delete the `.p12` file afterwards. If the certificate ever leaks, withdraw it in your developer account.

## The Mac as a build machine (later, optional)

GitHub's own Macs cost ten times the minutes on a private repository. The other way is your Mac as a runner: a small program from GitHub (the repository's Settings, Actions, Runners, New self-hosted runner, macOS, ARM64) that waits for a release and builds it.

Know what that means before saying yes: every run executes this repository's workflow on your Mac, as the user the runner was installed under. Give it a macOS user of its own, without your files and without administrator rights. The Mac has to be on when a release is made. On your own runner the certificate can stay in that user's Keychain, and `APPLE_CERTIFICATE` is not needed.
