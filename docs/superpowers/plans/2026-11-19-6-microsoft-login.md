# Microsoft Login Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `portal login` signs in with a real Microsoft account by device code, after which `portal join` and the window start the game with that account, so online-mode servers can be joined.

**Architecture:** `auth.rs` holds the chain as one small function per step (device code, Microsoft token, Xbox Live, XSTS, Minecraft, entitlement, profile), each a request plus a parser tested on a saved answer. Only the Microsoft refresh token is kept, in the Windows Credential Manager; everything else is fetched again when a game starts.

**Tech Stack:** as plans 1 and 4.

**Spec:** `docs/superpowers/specs/2026-11-19-spawnpoint-portal-design.md` (piece 8 of "Core, piece by piece"). Builds on plans 1 to 5.

**Blocked until:** Mojang has approved the Azure app the owner submitted on 19 November 2026. Without the approval `login_with_xbox` answers 403 for every account. Task 1 to 3 can be built and tested before it; Task 4's run check cannot pass.

## Global Constraints

All earlier plans', plus:

- Device code flow against the `consumers` tenant, scope `XboxLive.signin offline_access`.
- The client ID is a constant in the code (a public client, no secret). The owner supplies it from their Azure app registration.
- One account. Signing in again replaces it.
- Only the refresh token is stored: Credential Manager, service `SpawnpointPortal`, account `microsoft`. Microsoft gives a new refresh token with every refresh; the new one replaces the old at once.
- No Microsoft, Xbox or Minecraft token is ever printed, logged, written to a file, put in an error message or sent to the webview. The Minecraft access token goes to the game's arguments and nowhere else.
- Hand-written: no auth crate.
- Endpoints and field names below were read in November 2026; check Microsoft's device-code page and the community's Minecraft authentication reference before building, and let a saved real answer win over this plan.

## Review Focus

- The refresh token has expired or was revoked (unused for 90 days, password changed): "sign in again", and the game is not started with a stale or offline profile behind the player's back. Pinned in Task 3.
- An account without an Xbox profile, a child account, or a country without Xbox Live: the sentence Microsoft's code stands for, not a number. Pinned in Task 2.
- A Microsoft account that does not own Minecraft, or owns it but never chose a player name: said in those words. Pinned in Task 2.
- The player closes the code prompt or lets it time out: the sign-in ends cleanly and can be started again. Pinned in Task 1.
- No network when Play is clicked while signed in: an error about the network, not a silent offline start (an online-mode server would refuse it anyway, confusingly). Pinned in Task 3.

## File Structure

```
crates/core/src/auth.rs        the whole chain
crates/core/src/main.rs        + login, logout, whoami; launch and join use the account
src-tauri/src/commands.rs      + account commands
src/views/Account.vue          sign in, sign out
src/api.ts
crates/core/tests/fixtures/auth-*.json
```

---

### Task 1: Device code and Microsoft token

**Files:**
- Create: `crates/core/src/auth.rs`
- Create: `crates/core/tests/fixtures/auth-devicecode.json`, `auth-token.json`, `auth-token-pending.json`

**Interfaces:**
- Produces:
  - `pub const CLIENT_ID: &str = "<from the owner>";`
  - `pub struct DeviceCode { pub user_code: String, pub verification_uri: String, pub expires_in: u64, pub interval: u64, #[serde(skip_serializing)] pub device_code: String }` (the window shows the first four; `device_code` stays in Rust).
  - `pub struct MsTokens { pub access_token: String, pub refresh_token: String }` (no `Serialize`, no `Debug` derive).
  - `pub enum Poll { Pending, SlowDown, Done(MsTokens) }`
  - `pub fn parse_token_answer(status: u16, body: &str) -> Result<Poll>`
  - `pub async fn device_code(client: &Client) -> Result<DeviceCode>`: `POST https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode`, form `client_id`, `scope`.
  - `pub async fn poll(client: &Client, code: &DeviceCode) -> Result<Poll>`: `POST .../consumers/oauth2/v2.0/token`, form `grant_type=urn:ietf:params:oauth:grant-type:device_code`, `client_id`, `device_code`.
  - `pub async fn refresh(client: &Client, refresh_token: &str) -> Result<MsTokens>`: same URL, `grant_type=refresh_token`, `client_id`, `refresh_token`, `scope`.
  - New error: `Error::SignIn(String)`: the player has to sign in (again); its text is shown as it is.

`parse_token_answer`: 200 → `Done`. Otherwise by the body's `error`: `authorization_pending` → `Pending`; `slow_down` → `SlowDown` (the caller adds 5 s to its interval); `authorization_declined` → `Error::SignIn("the sign-in was declined")`; `expired_token` → `Error::SignIn("the code expired; start again")`; `invalid_grant` → `Error::SignIn("the saved sign-in is no longer valid; sign in again")`; anything else → `Error::Other` with `error` only, never `error_description` (it can echo request data).

Forms are sent with reqwest's `.form()`.

- [ ] **Step 1: Save the fixtures.** With the real client ID, `curl` the device code endpoint and the token endpoint once before approving (pending) and once after; in `auth-token.json` replace both tokens by `"ACCESS"` and `"REFRESH"` before saving.
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn device_code_parses_and_hides_its_secret() {
    let d: DeviceCode = serde_json::from_str(include_str!("../tests/fixtures/auth-devicecode.json")).unwrap();
    assert!(d.verification_uri.starts_with("https://"));
    assert!(!serde_json::to_string(&d).unwrap().contains(&d.device_code));
}
#[test] fn token_answers() {
    assert!(matches!(parse_token_answer(400, include_str!("../tests/fixtures/auth-token-pending.json")).unwrap(), Poll::Pending));
    assert!(matches!(parse_token_answer(400, r#"{"error":"slow_down"}"#).unwrap(), Poll::SlowDown));
    assert!(matches!(parse_token_answer(200, include_str!("../tests/fixtures/auth-token.json")).unwrap(), Poll::Done(t) if t.refresh_token == "REFRESH"));
    for e in ["authorization_declined", "expired_token", "invalid_grant"] {
        assert!(matches!(parse_token_answer(400, &format!(r#"{{"error":"{e}"}}"#)), Err(Error::SignIn(_))));
    }
    let other = parse_token_answer(400, r#"{"error":"invalid_client","error_description":"SECRET-ISH"}"#).unwrap_err().to_string();
    assert!(other.contains("invalid_client") && !other.contains("SECRET-ISH"));
}
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Sign in to Microsoft by device code`.

---

### Task 2: Xbox Live, XSTS, Minecraft

**Files:**
- Modify: `crates/core/src/auth.rs`
- Create: `crates/core/tests/fixtures/auth-xbl.json`, `auth-xsts.json`, `auth-xsts-error.json`, `auth-mc-login.json`, `auth-entitlements.json`, `auth-profile.json` (tokens replaced by placeholders)

**Interfaces:**
- Consumes: `launch::Profile`.
- Produces:
  - `pub struct Xbox { pub token: String, pub user_hash: String }` (`Token` and `DisplayClaims.xui[0].uhs`; no `Serialize`, no `Debug`).
  - `pub fn parse_xbox(status: u16, body: &str) -> Result<Xbox>`
  - `pub async fn xbox_live(client: &Client, ms_access_token: &str) -> Result<Xbox>`: `POST https://user.auth.xboxlive.com/user/authenticate`, JSON `{ "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": "d=<token>" }, "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT" }`.
  - `pub async fn xsts(client: &Client, xbl_token: &str) -> Result<Xbox>`: `POST https://xsts.auth.xboxlive.com/xsts/authorize`, JSON `{ "Properties": { "SandboxId": "RETAIL", "UserTokens": [<token>] }, "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT" }`.
  - `pub async fn minecraft_token(client: &Client, xsts: &Xbox) -> Result<String>`: `POST https://api.minecraftservices.com/authentication/login_with_xbox`, JSON `{ "identityToken": "XBL3.0 x=<user_hash>;<token>" }` → `access_token`. A 403 is `Error::Other("Mojang has not approved this launcher yet")`.
  - `pub fn owns_minecraft(entitlements_json: &str) -> Result<bool>`: `items` holds a `name` of `game_minecraft` or `product_minecraft`.
  - `pub async fn owns(client: &Client, mc_token: &str) -> Result<bool>`: `GET https://api.minecraftservices.com/entitlements/mcstore`, bearer.
  - `pub fn parse_profile(status: u16, body: &str, mc_token: &str) -> Result<Profile>`: 200 → `Profile { name, uuid: id, access_token: mc_token, user_type: "msa" }`; 404 → `Error::SignIn("this account has no Minecraft player name yet; start the official launcher once to choose one")`.
  - `pub async fn profile(client: &Client, mc_token: &str) -> Result<Profile>`: `GET https://api.minecraftservices.com/minecraft/profile`, bearer.

`parse_xbox` on a 401 reads `XErr`: `2148916233` → `Error::SignIn("this Microsoft account has no Xbox profile; make one at xbox.com first")`; `2148916235` → `"Xbox Live is not available in this account's country"`; `2148916236` and `2148916237` → `"this account needs adult verification on the Xbox site"`; `2148916238` → `"this is a child account; an adult has to add it to a Microsoft family"`; any other → `"Xbox Live refused the sign-in (<XErr>)"`.

`Profile` already derives `Serialize` and `Debug` (plan 1) and now carries a real token: remove both derives from `Profile` and fix what breaks (the window gets an `AccountView`, Task 4).

- [ ] **Step 1: Save the fixtures** by walking the chain once with `curl` (the last three need Mojang's approval; until then write them by hand after the reference and replace them with real answers in Task 4).
- [ ] **Step 2: Write the failing tests.**

```rust
#[test] fn xbox_answers() {
    let x = parse_xbox(200, include_str!("../tests/fixtures/auth-xbl.json")).unwrap();
    assert!(!x.user_hash.is_empty() && !x.token.is_empty());
    let e = parse_xbox(401, include_str!("../tests/fixtures/auth-xsts-error.json")).unwrap_err();
    assert!(matches!(e, Error::SignIn(_)));
    assert!(parse_xbox(401, r#"{"XErr":2148916238}"#).unwrap_err().to_string().contains("child account"));
    assert!(parse_xbox(401, r#"{"XErr":2148916233}"#).unwrap_err().to_string().contains("Xbox profile"));
    assert!(parse_xbox(401, r#"{"XErr":1}"#).unwrap_err().to_string().contains("(1)"));
}
#[test] fn ownership() {
    assert!(owns_minecraft(include_str!("../tests/fixtures/auth-entitlements.json")).unwrap());
    assert!(!owns_minecraft(r#"{"items":[]}"#).unwrap());
}
#[test] fn profiles() {
    let p = parse_profile(200, include_str!("../tests/fixtures/auth-profile.json"), "MC").unwrap();
    assert_eq!((p.user_type.as_str(), p.access_token.as_str(), p.uuid.len()), ("msa", "MC", 32));
    assert!(matches!(parse_profile(404, "{}", "MC"), Err(Error::SignIn(_))));
}
```

- [ ] **Step 3:** Run, see them fail; implement; run: PASS.
- [ ] **Step 4: Commit** `Walk the Xbox and Minecraft sign-in chain`.

---

### Task 3: The account

**Files:**
- Modify: `crates/core/src/auth.rs`, `crates/core/src/settings.rs`

**Interfaces:**
- Consumes: `secrets`, Tasks 1 and 2.
- Produces:
  - `pub async fn finish_sign_in(client: &Client, paths: &Paths, tokens: MsTokens) -> Result<String>`: stores the refresh token, walks the chain, checks `owns` (false: `Error::SignIn("this Microsoft account does not own Minecraft: Java Edition")`, and the stored token is deleted again), stores the player name in `Settings.account_name`, returns the name.
  - `pub async fn current_profile(client: &Client, paths: &Paths) -> Result<Profile>`: no stored refresh token → `Profile::offline(settings.player_name)`. Otherwise `refresh` (storing the new refresh token before anything else), then the chain. `Error::SignIn` from `refresh` deletes the stored token and `account_name` and is returned; any other error (network) is returned as it is and nothing is deleted. It never falls back to offline when a token is stored.
  - `pub fn sign_out(paths: &Paths) -> Result<()>`: deletes the token and `account_name`.
  - `Settings` gains `pub account_name: Option<String>` (`#[serde(default)]`).

The chain itself is one private `async fn chain(client, ms_access_token) -> Result<Profile>` used by both.

- [ ] **Step 1: Write the failing tests** (the local test server from `download::testing`; `auth.rs` takes its three hosts from private constants that tests override through one private `struct Hosts` parameter of `chain` and `refresh`):

```rust
#[tokio::test] async fn signed_out_is_offline() {
    // no secret stored -> current_profile is Profile::offline of the settings name; no request made
}
#[tokio::test] async fn a_dead_refresh_token_asks_to_sign_in_again() {
    // secret stored; token endpoint answers 400 invalid_grant -> Err(SignIn); the secret and account_name are gone
}
#[tokio::test] async fn a_network_error_keeps_the_sign_in() {
    // secret stored; hosts point at a closed port -> Err that is not SignIn; the secret is still there
}
#[tokio::test] async fn a_new_refresh_token_replaces_the_old() {
    // token endpoint answers REFRESH2, the next step answers 500 -> Err; the stored secret is REFRESH2
}
```

These tests use the Credential Manager account `microsoft-test`: `auth.rs` reads the account name from the same `Hosts` struct.

- [ ] **Step 2:** Run, see them fail; implement; run: PASS.
- [ ] **Step 3: Commit** `Keep one signed-in account`.

---

### Task 4: Commands, window, and the real check

**Files:**
- Modify: `crates/core/src/main.rs`, `src-tauri/src/commands.rs`, `src/api.ts`, `src/views/Account.vue`, `README.md`

**Interfaces:**
- Produces:
  - Command line: `portal login` (prints `Go to <verification_uri> and enter <user_code>`, polls every `interval` seconds, adding 5 on `SlowDown`, until done or `expires_in`; then `signed in as <name>`), `portal logout`, `portal whoami`. `launch` and `join` use `auth::current_profile`; `--name` only applies when signed out. The command line never prints the arguments it starts Java with.
  - Tauri: `account() -> AccountView { name: Option<String>, offline_name: String }`; `account_sign_in_start() -> DeviceCode` (kept in `AppState`); `account_sign_in_poll() -> Option<String>` (the name when done; `Pending` and `SlowDown` both answer `None`, and the window polls every `interval + 5` seconds so that a slow-down never comes); `account_sign_out()`. `join` and `instance_launch` take their profile from `auth::current_profile`.
  - `Account.vue`: signed out: the offline name field (as before) and a "Sign in with Microsoft" button, which shows the code in large type, a button that opens `verification_uri` (`openPage`), and waits; signed in: the player name and "Sign out".

- [ ] **Step 1:** Implement the command line part; run `cargo test --workspace`: PASS.
- [ ] **Step 2: Check by running, once Mojang has approved.**

Run: `cargo run -p portal-core -- login`, enter the code in a browser.
Expected: `signed in as <the owner's player name>`. `cmdkey /list | findstr SpawnpointPortal` shows the `microsoft` entry.

Run: `portal whoami`, then `portal join <an online-mode server>`
Expected: the name; the game starts with the owner's skin and joins the server.

Run: `portal logout`, `portal whoami`
Expected: `not signed in`; the Credential Manager entry is gone.

- [ ] **Step 3:** Replace the hand-written fixtures of Task 2 by the real answers (tokens replaced by placeholders); `cargo test`: PASS.
- [ ] **Step 4:** Implement the Tauri commands and the Account view; `npx vue-tsc --noEmit`: clean. Check in `npm run tauri dev`: sign in, see the name, join an online-mode server from the Servers view, sign out. With the developer tools: no answer of the four commands contains a token or the `device_code`.
- [ ] **Step 5:** README: signing in, and that one account is kept.
- [ ] **Step 6: Commit** `Sign in with Microsoft from the command line and the window`.
