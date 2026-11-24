# Trying the Linux side in WSL

Portal is built on Windows. Its Linux side can be tried on the same machine in WSL (Ubuntu 24.04), which can also show Linux windows on the Windows desktop. This is for quick checks while the Linux work is going on (`docs/superpowers/plans/2026-11-24-8-linux-and-macos.md`); it does not replace a real Linux desktop.

Run everything below in the Ubuntu terminal, not in Git Bash or PowerShell.

## Set up, once

System packages (asks for the Linux password):

```bash
sudo apt update
sudo apt install -y build-essential curl wget file pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev \
  gnome-keyring libsecret-1-0 dbus-x11 \
  libgl1 libglx-mesa0 mesa-utils libopenal1 libfuse2t64 gh
```

The first two lines build Portal, the third is the keyring for the sign-in, the last is for the game window, the AppImage and GitHub.

Rust, for this user (accept the default):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

Node for Linux. Without it WSL finds the Windows `npm`, which must not build a Linux program:

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
source ~/.bashrc
nvm install --lts
```

The code, on Linux's own disk. Under `/mnt/c` builds are very slow and would collide with the Windows build folder:

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
cargo test -p portal-core
glxinfo -B | grep -E "renderer|OpenGL version"
```

The renderer line says what the game will draw with. `llvmpipe` is drawing on the processor: the game can open, slowly, and says nothing about how it runs on a real Linux machine. A name with `D3D12` is the graphics card. When it says `llvmpipe`, ask for the card in the terminal the game is started from:

```bash
export GALLIUM_DRIVER=d3d12
export MESA_D3D12_DEFAULT_ADAPTER_NAME=NVIDIA   # only with more than one card
```

## Try a branch

```bash
cd ~/code/spawnpoint-portal
git fetch origin && git checkout <branch> && git pull
cargo test -p portal-core
```

On Linux Portal keeps its files in `~/.local/share/spawnpoint-portal`. Give a try a folder of its own, so it leaves nothing else behind:

```bash
export SPAWNPOINT_PORTAL_HOME=~/portal-try

cargo run -p portal-core -- java java-runtime-delta   # prints the Java it downloaded
cargo run -p portal-core -- files 1.21.1              # the game files of a version
```

Run the printed Java with `-version` to see that it starts. `~/portal-try` holds a few hundred megabytes and can be deleted at any time.

## Sign in and start the game

The sign-in is kept in the keyring, and WSL has no unlocked one. This gives one session a keyring of its own, kept in the try folder:

```bash
export XDG_DATA_HOME=~/portal-try/xdg
dbus-run-session -- bash
echo -n try | gnome-keyring-daemon --unlock --components=secrets
cargo run -p portal-core -- login          # enter the code it prints at microsoft.com/link
cargo run -p portal-core -- instance create Vanilla 1.21.1
cargo run -p portal-core -- launch vanilla
```

A new instance started with `launch --server` first shows Minecraft's accessibility screen and joins after a click; `portal join` turns that screen off. Run `logout` when done: the keyring's password is the word in the line above.

## What WSL does not show

- How the game performs, and whether a real graphics driver is happy.
- A desktop's own keyring and its Discord.
- How the window looks under a real window manager.
