# Panel Client: Pairing, Sync, Join, CurseForge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `portal panel add <address>` links a panel by code, `portal servers` lists the servers of every linked panel, and `portal join <server>` makes or updates that server's instance and starts the game connected to it.

**Architecture:** `panel.rs` is a thin typed client for `/api/v1`. `join::desired` is the pure decision (what a server reports → Minecraft version, loader, wanted files); the sync of plan 3 does the rest. Panel tokens live in the Windows Credential Manager through `secrets.rs`; a token is only ever sent to the panel it belongs to.

**Tech Stack:** as plan 1, plus `keyring`.

**Spec:** `docs/superpowers/specs/2026-11-19-spawnpoint-portal-design.md` ("Panel client", "Joining a server", "What the panel offers a launcher", "Rules of a sync", the CurseForge part of "Pack installers"). The panel's `openapi.json` is the reference: `panel/resources/api/openapi.json` on branch `feature/JKER_1.7.0_modpack_update_check` of `../spawnpoint`. Builds on plans 1 to 3.

## Global Constraints

All earlier plans', plus:

- The launcher talks only to `<panel address>/api/v1`, never to a node.
- A panel token is stored only in the Windows Credential Manager (service `SpawnpointPortal`, account `panel:<panel id>`), never in `panels.json`, never printed, never in an error message.
- The `Authorization: Bearer` header goes only on requests whose URL starts with that panel's own address. Download URLs the panel hands out (Modrinth, CurseForge CDNs) get no header.
- A panel address must be `https://`, or `http://` only when the caller passes `allow_http`.
- Servers are named by their `slug`. The Minecraft version is `game_version`, never `mc_version`.
- A sync runs on every join. Jars only from `client-mods`; no `config/` from the server.
- `openapi.json` is the reference: when it and this plan disagree, it wins.

## Review Focus

- A panel that was linked and has since revoked the token (401): "the panel <name> no longer accepts this launcher; link it again", not a raw status. Pinned in Task 2.
- A token without `servers:join` (403 on the launcher routes): the message names the ability and the page `<address>/settings/api-tokens`. Pinned in Task 2.
- A pack server whose pack is not installed yet (`modpack.version` null, or 409 from `client-mods`): "the server is still installing its pack", no instance damaged. Pinned in Task 4.
- A `client-mods` answer naming a file `..\..\x.jar`: refused by `safe_path` before anything is written. Pinned in Task 4.
- A panel that is offline while another is fine: `portal servers` still lists the other's servers and names the one that failed. Pinned in Task 6.

## File Structure

```
crates/core/src/secrets.rs           Windows Credential Manager
crates/core/src/panel.rs             panels.json, pairing, the API calls
crates/core/src/download.rs          + bearer on a Download
crates/core/src/join.rs              desired(), join()
crates/core/src/pack/curseforge.rs   manifest.json, overrides, file addresses through the panel
crates/core/src/main.rs              + panel, servers, join
```

---

### Task 1: Secrets and linked panels

**Files:**
- Create: `crates/core/src/secrets.rs`, `crates/core/src/panel.rs`
- Modify: `crates/core/Cargo.toml` (add `keyring` with its Windows native feature; check the feature name in the crate's current README)

**Interfaces:**
- Produces, in `secrets.rs`: `pub fn set(account: &str, secret: &str) -> Result<()>`, `pub fn get(account: &str) -> Result<Option<String>>` (no entry: `None`), `pub fn delete(account: &str) -> Result<()>` (no entry: fine). Service name `SpawnpointPortal`. Add `Error::Secret(String)`.
- Produces, in `panel.rs`:
  - `pub struct Panel { pub id: String, pub address: String, pub name: String }`
  - `pub fn normalize_address(input: &str, allow_http: bool) -> Result<String>`: trims; adds `https://` when there is no scheme; removes a trailing `/` and a trailing `/api/v1`; `http://` without `allow_http` is `Error::Other("<address> is not https; pass --allow-http to link it anyway")`; anything else than http(s) is an error.
  - `pub fn list(paths: &Paths) -> Result<Vec<Panel>>` (no file: empty), `pub fn add(paths: &Paths, address: &str, name: &str, token: &str) -> Result<Panel>` (id: `instance::slug` of the host, numbered when taken; an address already linked is replaced, keeping its id; token to `secrets`), `pub fn remove(paths: &Paths, id: &str) -> Result<()>` (also deletes the token), `pub fn token(panel: &Panel) -> Result<String>` (missing: `Error::Other("the panel <name> has no token; link it again")`).

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn addresses() {
    assert_eq!(normalize_address(" panel.example.org/ ", false).unwrap(), "https://panel.example.org");
    assert_eq!(normalize_address("https://panel.example.org/api/v1/", false).unwrap(), "https://panel.example.org");
    assert!(normalize_address("http://10.0.0.5:8080", false).is_err());
    assert_eq!(normalize_address("http://10.0.0.5:8080", true).unwrap(), "http://10.0.0.5:8080");
    assert!(normalize_address("ftp://x", true).is_err());
}
#[test] fn secrets_round_trip() {
    // account "test:<random>": get -> None; set; get -> Some; delete; get -> None; delete again is Ok
}
#[test] fn panels_file_has_no_token() {
    let p = add(&paths, "https://panel.example.org", "Home", "tok-123").unwrap();
    assert!(!std::fs::read_to_string(paths.panels_file()).unwrap().contains("tok-123"));
    assert_eq!(token(&p).unwrap(), "tok-123");
    add(&paths, "https://panel.example.org", "Home again", "tok-456").unwrap();
    assert_eq!(list(&paths).unwrap().len(), 1);
    remove(&paths, &p.id).unwrap();
    assert!(token(&p).is_err() && list(&paths).unwrap().is_empty());
}
```

- [ ] **Step 2:** Run, see them fail; implement; run: PASS (these tests write to the real Credential Manager under test accounts and clean up).
- [ ] **Step 3: Commit** `Keep linked panels and their tokens`.

---

### Task 2: The API calls

**Files:**
- Modify: `crates/core/src/panel.rs`, `crates/core/src/error.rs`
- Create: `crates/core/tests/fixtures/panel-servers.json`, `panel-client-mods.json`

**Interfaces:**
- Produces:
  - `pub struct Pairing { pub code: String, pub secret: String, pub expires_at: String, pub interval: u64 }`
  - `pub struct PairToken { pub token: String, pub abilities: Vec<String>, pub user: PairUser }`, `PairUser { name: Option<String> }`
  - `pub async fn pair_start(client: &Client, address: &str, device_name: &str) -> Result<Pairing>`: `POST /pair` with `{ "device_name" }` (at most 60 characters), answer under `data`; 429 is `Error::Other("too many pairings started; wait a minute")`.
  - `pub async fn pair_poll(client: &Client, address: &str, pairing: &Pairing) -> Result<Option<PairToken>>`: `POST /pair/token` with `{ code, secret }`; 202 is `None`; 404 is `Error::Other("the pairing code expired or was already used")`.
  - `pub fn missing_abilities(abilities: &[String]) -> Vec<&'static str>`: which of `servers:read`, `servers:join` are absent.
  - `pub struct Server { pub slug: String, pub name: String, pub hostname: String, pub state: String, pub game_version: Option<String>, pub modpack: Option<Modpack>, pub loader: Option<ServerLoader>, pub players: Option<Players> }`, `Modpack { catalogue: String, project: String, version: Option<String>, version_name: Option<String> }`, `ServerLoader { name: String, version: Option<String> }`, `Players { online: u32, max: u32 }`.
  - `pub async fn servers(client: &Client, panel: &Panel) -> Result<Vec<Server>>` (`GET /servers`, list under `data`), `pub async fn server(client: &Client, panel: &Panel, slug: &str) -> Result<Server>` (`GET /servers/<slug>`; read `openapi.json` for whether it is wrapped in `data`).
  - `pub struct ClientMods { pub mods: Vec<ClientMod>, #[serde(default)] pub removed: Vec<String> }`, `ClientMod { file: String, sha512: String, size: u64, url: Option<String> }`
  - `pub async fn client_mods(client: &Client, panel: &Panel, slug: &str) -> Result<ClientMods>`: 404 is an empty `ClientMods` (a server that loads no mods); 409 is `Error::NotReady("the server is still installing its pack")`.
  - `pub fn client_mod_url(panel: &Panel, slug: &str, file: &str) -> String`: `<address>/api/v1/servers/<slug>/client-mods/<file percent-encoded>`.
  - `pub struct ClientPack { pub project: u64, pub file: u64, pub archive: Option<String>, pub page: String }` and `pub async fn client_pack(client, panel, slug) -> Result<ClientPack>`: 409 is `Error::Other("the panel <name> has no CurseForge key")`.
  - `pub struct PackFile { pub project: u64, pub file: u64, pub name: String, pub url: Option<String>, pub page: Option<String> }` and `pub async fn client_pack_files(client, panel, slug, files: &[u64]) -> Result<Vec<PackFile>>`: sends chunks of at most 2000 and joins the answers.
  - New errors: `Error::NotReady(String)`, `Error::Panel { panel: String, message: String }`.

One private `async fn call(...)` does every authenticated request and maps: 401 → `Error::Panel` "no longer accepts this launcher; link it again"; 403 → `Error::Panel` "this launcher's token may not do that: it needs servers:read and servers:join, granted at <address>/settings/api-tokens"; 502 → `Error::Panel` "the panel's host service is not answering"; a failed connection → `Error::Panel` "cannot be reached". None of them contains the token. Percent-encoding is written by hand (everything but `A-Za-z0-9-._~`).

- [ ] **Step 1: Save the fixtures** from the owner's panel with a paired token (`curl -H "Authorization: Bearer ..."`), then replace hostnames and names by made-up ones. `panel-servers.json` should hold one pack server, one plain modded server and one Paper or vanilla server; if the panel lacks one, write that entry by hand after the schema in `openapi.json`. Do not commit a token.
- [ ] **Step 2: Write the failing tests** (the local test server from `download::testing` answers a given status and body):

```rust
#[test] fn parses_servers()        // 3 servers; the pack one has modpack.catalogue and loader.name; the vanilla one has both None
#[test] fn parses_client_mods()    // url None for one mod; removed present
#[test] fn abilities() {
    assert_eq!(missing_abilities(&["servers:read".into()]), vec!["servers:join"]);
    assert!(missing_abilities(&["servers:read".into(), "servers:join".into(), "x".into()]).is_empty());
}
#[test] fn mod_urls_are_encoded() {
    assert_eq!(client_mod_url(&panel, "atm10", "my mod+1.jar"), "https://panel.example.org/api/v1/servers/atm10/client-mods/my%20mod%2B1.jar");
}
#[tokio::test] async fn pending_pairing_is_none()            // 202 -> Ok(None)
#[tokio::test] async fn a_revoked_token_says_link_again()    // 401 -> Error::Panel, message contains "link it again", not the token
#[tokio::test] async fn a_missing_ability_names_the_page()   // 403 -> message contains "servers:join" and "/settings/api-tokens"
#[tokio::test] async fn no_mods_is_empty_and_409_is_not_ready()
#[tokio::test] async fn file_ids_go_in_chunks_of_2000()      // 4500 ids -> the server saw 3 requests
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Talk to a panel's API`.

---

### Task 3: Downloads with a panel token

**Files:**
- Modify: `crates/core/src/download.rs`, and every place that builds a `Download` (add `bearer: None`)

**Interfaces:**
- Produces: `Download` gains `pub bearer: Option<String>`; `fetch` sends `Authorization: Bearer <it>` when set. `sync::Origin` gains `PanelFile { url: String, bearer: String }`, which `sync::apply` turns into a `Download` with the bearer. `Download`, `Wanted` and `Origin` must not derive `Serialize` or print the bearer in `Debug` (write `Debug` for `Origin` by hand): they are the only types that carry a token.

- [ ] **Step 1: Write the failing test:** `a_bearer_is_sent_only_when_set`: the test server records headers; one fetch with `bearer: Some("t")` shows `authorization: Bearer t`, one with `None` shows no such header.
- [ ] **Step 2:** Run, see it fail; implement; run all tests: PASS.
- [ ] **Step 3: Commit** `Send a panel token with panel downloads only`.

---

### Task 4: Deciding what a server needs

**Files:**
- Create: `crates/core/src/join.rs`

**Interfaces:**
- Consumes: `Server`, `ClientMods`, `PackInstall`, `Wanted`, `Origin`, `Loader`, `LoaderKind`, `client_mod_url`, `safe_path`.
- Produces:
  - `pub struct Desired { pub minecraft: String, pub loader: Option<Loader>, pub files: Vec<Wanted> }`
  - `pub fn desired(panel: &Panel, token: &str, server: &Server, pack: Option<PackInstall>, mods: ClientMods) -> Result<Desired>`

Rules:
- `minecraft`: `server.game_version`; `None` is `Error::NotReady("the panel does not know this server's Minecraft version yet")`.
- `loader`: with a pack, the pack's. Otherwise from `server.loader` (`LoaderKind::parse(name)`, version as sent, possibly `None`); `None` for a server without one.
- `files`: the pack's files, minus those whose path is `mods/<name>` for a `name` in `mods.removed`; then one per `mods.mods`: path `mods/<file>` (through `safe_path`; a `file` with `/` or `\` is refused), hash `Sha512`, from `Url(url)` when the panel gave one, else `PanelFile { url: client_mod_url(..), bearer: token }`. A server mod replaces a pack file of the same path.
- A server with a `modpack` but `pack` `None` is a programming error in the caller; `join` (Task 6) never does that.

Before matching on loader names: list the owner's servers of each kind (`portal servers` after Task 6, or `curl` now) and note what the panel really sends in `loader.name`. If a spelling is not one `LoaderKind::parse` accepts, extend `parse` and its test in plan 2's file.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn vanilla_server()       // no loader, no pack, empty mods -> files empty, loader None
#[test] fn plain_modded_server() {
    // loader fabric (version None), mods [a.jar with url, b.jar without]
    // -> loader Fabric/None; a: Origin::Url, no bearer anywhere in it; b: Origin::PanelFile with the panel's url and token
}
#[test] fn pack_server_applies_the_servers_changes() {
    // pack files [mods/x.jar, mods/y.jar, config/c.toml]; removed ["y.jar"]; mods [z.jar, x.jar (other hash)]
    // -> paths {mods/x.jar (the server's hash), mods/z.jar, config/c.toml}; loader is the pack's
}
#[test] fn unknown_version_is_not_ready()      // game_version None -> Err(NotReady)
#[test] fn a_mod_name_with_a_path_is_refused() // file "..\\..\\x.jar" and "sub/x.jar" -> Err
#[test] fn quilt_server_is_unsupported()
```

- [ ] **Step 2:** Run, see them fail; implement; run: PASS.
- [ ] **Step 3: Commit** `Decide what a server's instance needs`.

---

### Task 5: The CurseForge installer

**Files:**
- Create: `crates/core/src/pack/curseforge.rs`
- Create: `crates/core/tests/fixtures/curseforge-manifest.json`

**Interfaces:**
- Consumes: `client_pack`, `client_pack_files`, `PackFile`, `PackInstall`, `Wanted`, `Origin`, `fetch`.
- Produces:
  - `pub struct Manifest { pub minecraft: String, pub loader: Option<Loader>, pub overrides: String, pub files: Vec<u64> }`
  - `pub fn parse_manifest(json: &str) -> Result<Manifest>`: `minecraft.version`; the loader from the entry of `minecraft.modLoaders` with `primary: true` (else the first), whose `id` is `<name>-<version>` split at the first `-` (`forge-47.2.0`, `neoforge-21.1.90`, `fabric-0.16.5`); `overrides` (default `overrides`); the `fileID` of every entry of `files` whose `required` is not `false`.
  - `pub fn wanted(manifest: &Manifest, files: &[PackFile], archive: &Path) -> Result<PackInstall>`: each `PackFile` becomes a `Wanted` at `mods/<name>` for a name ending in `.jar`, `resourcepacks/<name>` for one ending in `.zip` (the panel sends no file type; mark this line `// ponytail: by extension; ask the panel for the class when a shader pack lands in the wrong folder`); from `Url(url)` or, without one, `Manual { name, page }`; no hash. Then every file under `<overrides>/` in the archive as `Origin::Bytes`.
  - `pub async fn resolve(client: &Client, paths: &Paths, panel: &Panel, slug: &str) -> Result<PackInstall>`: `client_pack`; `archive` null is `Error::Other("this pack's author forbids outside downloads; fetch it from <page>")`; download the archive to `meta/packs/curseforge-<project>-<file>.zip` without a bearer; `parse_manifest`; `client_pack_files`; `wanted`.

- [ ] **Step 1: Save the fixture:** `manifest.json` from a real CurseForge pack zip, `files` trimmed to three, one edited to `"required": false`.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn manifest() {
    let m = parse_manifest(include_str!("../../tests/fixtures/curseforge-manifest.json")).unwrap();
    assert_eq!(m.files.len(), 2);
    assert!(m.loader.unwrap().version.is_some());
}
#[test] fn files_and_overrides() {
    // PackFiles: a.jar with url, b.jar with url None and page "https://www.curseforge.com/...", pack.zip with url;
    // archive built in the test with overrides/config/x.toml
    // -> mods/a.jar Url; mods/b.jar Manual with that page; resourcepacks/pack.zip; config/x.toml Bytes
}
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Read a CurseForge pack through the panel`.

---

### Task 6: Join, and the commands

**Files:**
- Modify: `crates/core/src/join.rs`, `crates/core/src/pack.rs`, `crates/core/src/main.rs`, `README.md`

**Interfaces:**
- Produces:
  - `pub fn instance_for(paths: &Paths, panel: &Panel, server: &Server) -> Result<Instance>`: the instance whose `source` is `Server` with this panel id and slug, else a new one named after the server.
  - `pub async fn sync(client: &Client, paths: &Paths, panel: &Panel, slug: &str, progress: &ProgressTx) -> Result<(Instance, Server, Vec<Manual>)>`: `server`; `instance_for`; for a pack server (`modpack` set; its `version` null is `Error::NotReady("the server is still installing its pack")`) resolve the pack: `ftb` and `modrinth` with plan 3's `pack::resolve`, `curseforge` with `curseforge::resolve`; `client_mods`; `desired`; set the instance's `minecraft`, `loader` and `source.pack`; `sync::plan` and `sync::apply`.
  - `pub async fn join(client: &Client, paths: &Paths, panel: &Panel, slug: &str, profile: Profile, progress: &ProgressTx) -> Result<(Child, Vec<Manual>)>`: `sync`, then `play::start_instance` with `server: Some(server.hostname)`.
  - `pub async fn all_servers(client: &Client, paths: &Paths) -> Vec<(Panel, Result<Vec<Server>>)>`: every linked panel asked at the same time; one panel's failure does not hide the others.

- [ ] **Step 1: Write the failing test** for `instance_for`: called twice for the same panel and server it returns the same id; for the same slug on another panel, another instance.
- [ ] **Step 2:** Run, see it fail; implement Task 6; run all tests: PASS.
- [ ] **Step 3: Commands.**
  - `portal panel add <address> [--allow-http]`: `pair_start` with the device name `Spawnpoint Portal on <COMPUTERNAME>`, print `Approve the code <code> at <address>/settings/api-tokens`, poll every `interval` seconds until a token or `expires_at`, then `add` (name: the address's host) and warn when `missing_abilities` is not empty, naming them and that page.
  - `portal panel list`, `portal panel remove <id>`.
  - `portal servers`: per panel its servers as `<panel id>/<slug>  <name>  <state>  <game_version>  <loader or pack>`; a failed panel as `<panel id>: <error>`.
  - `portal sync <panel id>/<slug>` and `portal join <panel id>/<slug> [--name <player>]`; the panel part may be left out when one panel is linked. Manual files are printed as in plan 3.
- [ ] **Step 4: Check against the owner's running panel** (it must run 1.7.0 or newer).

Run: `cargo run -p portal-core -- panel add <address>`, approve the code in the panel.
Expected: `linked <name>`; `cmdkey /list | findstr SpawnpointPortal` shows an entry; `panels.json` holds no token.

Run: `portal servers`
Expected: the owner's servers with real versions; note the `loader.name` spellings (Task 4).

Run: `portal join <the FTB server>` on a server in offline mode, or `portal sync` on an online-mode one (joining those waits for plan 6).
Expected: an instance named after the server; a second `portal sync` downloads nothing. Add a mod to the server in the panel, sync again: only that jar arrives. Remove it there, sync: it is gone, and a jar added by hand with `instance add-mod` is still there.

Run: revoke the token in the panel, then `portal servers`
Expected: `<panel id>: ... no longer accepts this launcher; link it again`.

- [ ] **Step 5:** Update the README's command list.
- [ ] **Step 6: Commit** `Link panels and join their servers`.

---

## Carried over from plans 1 to 3

Found while building and reviewing the earlier plans. Where a line here and a task above disagree, the line here describes the code as it is.

- Interfaces as built: `pack::install(client, paths, &mut instance, pack, progress)` plans first and only then sets the instance's version and loader, saves and applies; `join::sync` should build one `PackInstall` (pack files, minus `removed`, plus the server's mods) and call it, rather than a separate `Desired` type with its own copy of those steps. `play::start_instance` takes `memory_mb` (pass `DEFAULT_MEMORY_MB`). `loader::resolve` turns a loader without a version into one with it and checks both versions with `paths::check_id`.
- FTB paths are normalised by the reader (`mods/x.jar`, no `./`), so matching `removed` names against `mods/<name>` works; compare lower-cased, as the sync does.
- Unhashed files that come as bytes (archive overrides) are hashed by `sync::plan`, so they update when their content changes; the CurseForge overrides get that for free.
- `sync::apply`'s match on `Origin` names every variant: adding `Origin::PanelFile` will not compile until it is handled there.
- `download::Download` derives `Serialize` today; it must lose that when it gains `bearer`. Temporary files use `download::part_path` (`.portal-part`).
- The bearer goes only to the panel's own address, and redirects are followed by reqwest: a panel download that redirects to another host must not carry the header there. Either disable redirects for panel requests or check the final host.
- Anything from the panel that reaches a URL or a file name (slug, loader name and version, mod file names, pack ids) goes through `paths::check_id` or `sync::safe_path`, as pack data does.
- A hand-placed file is never taken for a managed one unless its hash proves it (ruling in plan 3); a write skipped because the player's file is in the way is silent today, and the window (plan 5) should list those.
- Nothing locks an instance during a sync; two syncs at once race on `instance.json`. The window's "already starting" guard (plan 5) covers the window; the command line is single-shot.
- Open decision for the owner (plan 5): a pack mod disabled by renaming it to `x.jar.disabled` comes back as `x.jar` on the next sync.
- Still open from earlier plans: vanilla assets hashed twice per modded launch; `spawn_blocking` for hashing and zip work; `CREATE_NO_WINDOW` for installer processors; `java::ensure` needing the network when installed; `Error` not `Serialize`; single-pass `launch::fill` before real tokens (plan 6); a size cap on archive overrides; old `.mrpack` files are never removed; `slug` can yield a Windows device name; `cargo fmt` not run.
