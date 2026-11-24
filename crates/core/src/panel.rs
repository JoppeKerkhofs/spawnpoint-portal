use reqwest::{Method, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::instance::slug;
use crate::paths::check_id;
use crate::sync::Origin;
use crate::{Error, Paths, Result, panel_client, secrets};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub address: String,
    pub name: String,
}

pub fn normalize_address(input: &str, allow_http: bool) -> Result<String> {
    let input = input.trim();
    let full = if input.contains("://") { input.to_string() } else { format!("https://{input}") };
    let (scheme, rest) = full.split_once("://").unwrap_or_default();
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(Error::Other(format!("{input} is not an http(s) address")));
    }
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    if authority.is_empty() || authority.contains('@') || rest.contains(['?', '#']) {
        return Err(Error::Other(format!("{input} must be a plain address: a host, an optional port and path, no login, query or fragment")));
    }
    let path = path.trim_end_matches('/');
    let cut = path.len().saturating_sub("/api/v1".len());
    let path = if path.len() >= 7 && path[cut..].eq_ignore_ascii_case("/api/v1") { &path[..cut] } else { path };
    let address = format!("{scheme}://{}{}", authority.to_ascii_lowercase(), path.trim_end_matches('/'));
    if scheme == "http" && !allow_http {
        return Err(Error::Other(format!("{address} is not https; pass --allow-http to link it anyway")));
    }
    Ok(address)
}

pub fn host(address: &str) -> &str {
    let rest = address.split_once("://").map_or(address, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest)
}

pub fn list(paths: &Paths) -> Result<Vec<Panel>> {
    match std::fs::read(paths.panels_file()) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

fn save(paths: &Paths, panels: &[Panel]) -> Result<()> {
    let file = paths.panels_file();
    std::fs::create_dir_all(&paths.root)?;
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(panels)?)?;
    std::fs::rename(&tmp, &file)?;
    Ok(())
}

fn account(id: &str) -> String {
    format!("panel:{id}")
}

pub fn add(paths: &Paths, address: &str, name: &str, token: &str, allow_http: bool) -> Result<Panel> {
    let address = normalize_address(address, allow_http)?;
    let mut panels = list(paths)?;
    let panel = if let Some(old) = panels.iter_mut().find(|p| p.address == address) {
        old.name = name.to_string();
        old.clone()
    } else {
        let authority = host(&address);
        let host = match authority.rsplit_once(':') {
            Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) => h,
            _ => authority,
        };
        let base = slug(host);
        let mut id = base.clone();
        let mut n = 2;
        while panels.iter().any(|p| p.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let panel = Panel { id, address, name: name.to_string() };
        panels.push(panel.clone());
        panel
    };
    secrets::set(&account(&panel.id), token)?;
    save(paths, &panels)?;
    Ok(panel)
}

pub fn remove(paths: &Paths, id: &str) -> Result<()> {
    let mut panels = list(paths)?;
    panels.retain(|p| p.id != id);
    save(paths, &panels)?;
    secrets::delete(&account(id))
}

pub fn token(panel: &Panel) -> Result<String> {
    secrets::get(&account(&panel.id))?
        .ok_or_else(|| Error::Other(format!("the panel {} has no token; link it again", panel.name)))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pairing {
    pub code: String,
    #[serde(skip_serializing)]
    pub secret: String,
    pub expires_at: String,
    pub interval: u64,
    // Where a signed-in person allows this device; a panel before 1.9.4 sends none.
    #[serde(default)]
    pub approve_path: Option<String>,
}

impl std::fmt::Debug for Pairing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pairing").field("code", &self.code).field("secret", &"***").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairUser {
    pub name: Option<String>,
}

// Holds the token: no Debug and no Serialize on purpose.
#[derive(Clone, Deserialize)]
pub struct PairToken {
    pub token: String,
    pub abilities: Vec<String>,
    pub user: PairUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modpack {
    pub catalogue: String,
    pub project: String,
    pub version: Option<String>,
    pub version_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerLoader {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Players {
    pub online: u32,
    pub max: u32,
    #[serde(default, deserialize_with = "clean_names")]
    pub names: Vec<String>,
}

fn cut(s: String, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => s[..i].to_string(),
        None => s,
    }
}

fn clean_names<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    // A null list or an entry that is not text must not cost the panel its whole server list.
    let names = Option::<Vec<serde_json::Value>>::deserialize(d)?.unwrap_or_default();
    Ok(names.into_iter().filter_map(|n| n.as_str().map(|s| cut(s.to_string(), 64))).take(200).collect())
}

fn clean_detail<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.map(|s| cut(s, 300)))
}

const LOGO_HOSTS: [&str; 4] = ["https://cdn.modrinth.com/", "https://media.forgecdn.net/", "https://cdn.feed-the-beast.com/", "https://horizon-rust.github.io/horizon-mods/"];
const PNG_PREFIX: &str = "data:image/png;base64,";

/// The one rule for a picture that reaches an `<img src>`: a small PNG data URI or a known logo address.
pub fn picture(s: &str) -> Option<&str> {
    let ok = match s.strip_prefix(PNG_PREFIX) {
        Some(data) => s.len() <= 200_000 && data.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')),
        None => {
            LOGO_HOSTS.iter().any(|h| s.starts_with(h))
                && s.len() <= 300
                && s.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'<' | b'>' | b'\\'))
        }
    };
    ok.then_some(s)
}

fn clean_picture<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.filter(|s| picture(s).is_some()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub slug: String,
    pub name: String,
    pub hostname: String,
    pub state: String,
    pub game_version: Option<String>,
    pub modpack: Option<Modpack>,
    pub loader: Option<ServerLoader>,
    pub players: Option<Players>,
    #[serde(default, deserialize_with = "clean_picture")]
    pub picture: Option<String>,
    #[serde(default, deserialize_with = "clean_detail")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMod {
    pub file: String,
    pub sha512: String,
    pub size: u64,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientMods {
    pub mods: Vec<ClientMod>,
    #[serde(default)]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientPack {
    pub project: u64,
    pub file: u64,
    pub archive: Option<String>,
    pub page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFile {
    pub project: u64,
    pub file: u64,
    pub name: String,
    pub url: Option<String>,
    pub page: Option<String>,
}

pub fn missing_abilities(abilities: &[String]) -> Vec<&'static str> {
    ["servers:read", "servers:join"].into_iter().filter(|a| !abilities.iter().any(|x| x == a)).collect()
}

fn encode(part: &str) -> String {
    part.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn panel_error(panel: &str, message: impl Into<String>) -> Error {
    Error::Panel { panel: panel.into(), message: message.into() }
}

// The page to open for a pairing, and whether it is the one where signing in is all it takes.
// The panel's path is only used when it is exactly this code's: it must not send Portal elsewhere.
pub fn approve_page(address: &str, pairing: &Pairing) -> (String, bool) {
    let own = format!("/pair/{}", pairing.code);
    let plain = pairing.code.bytes().all(|b| b.is_ascii_alphanumeric());
    if plain && pairing.approve_path.as_deref() == Some(own.as_str()) {
        (format!("{address}{own}"), true)
    } else {
        (format!("{address}/settings/api-tokens"), false)
    }
}

// ponytail: Windows and Linux only; macOS has neither source and will need its own.
pub fn computer_name() -> Option<String> {
    let name = if cfg!(windows) { std::env::var("COMPUTERNAME").ok() } else { std::fs::read_to_string("/proc/sys/kernel/hostname").ok() };
    name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

pub fn device_name(computer: Option<String>) -> String {
    let unnamed = if cfg!(windows) { "this PC" } else { "this computer" };
    let name = format!("Spawnpoint Portal on {}", computer.as_deref().unwrap_or(unnamed));
    name.chars().take(60).collect()
}

// The token goes in the header only, and only through the no-redirect client.
async fn send(
    name: &str,
    address: &str,
    token: Option<&str>,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    let mut req = panel_client().request(method, format!("{address}/api/v1{path}"));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    if let Some(body) = body {
        req = req.json(&body);
    }
    let resp = req.send().await.map_err(|_| Error::Unreachable { panel: name.into() })?;
    let status = resp.status();
    match status.as_u16() {
        401 => Err(panel_error(name, "no longer accepts this launcher; link it again")),
        403 => {
            let said = resp.json::<serde_json::Value>().await.ok().and_then(|v| v["message"].as_str().map(String::from)).unwrap_or_default();
            // The panel's own words when the account lacks two-factor; the token is fine then.
            Err(panel_error(name, if said.contains("two-factor") {
                said.chars().take(200).collect()
            } else {
                format!("this launcher's token may not do that: it needs servers:read and servers:join, granted at {address}/settings/api-tokens")
            }))
        }
        502 => Err(panel_error(name, "the panel's host service is not answering")),
        _ if status.is_redirection() => Err(panel_error(name, "answered with a redirect; check the panel address")),
        _ => Ok(resp),
    }
}

async fn call(
    panel: &Panel,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    let token = token(panel)?;
    send(&panel.name, &panel.address, Some(&token), method, path, body).await
}

fn ok(resp: Response) -> Result<Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    Err(Error::Status { url: resp.url().to_string(), status: resp.status().as_u16() })
}

// Answers come as {"data": ...}; some are not documented, so a bare value is accepted too.
async fn data<T: DeserializeOwned>(resp: Response) -> Result<T> {
    let mut v: serde_json::Value = ok(resp)?.json().await?;
    if let Some(inner) = v.get_mut("data") {
        v = inner.take();
    }
    Ok(serde_json::from_value(v)?)
}

pub async fn pair_start(address: &str, device_name: &str) -> Result<Pairing> {
    let name: String = device_name.chars().take(60).collect();
    let body = serde_json::json!({ "device_name": name, "abilities": ["servers:read", "servers:join"] });
    let resp = send(address, address, None, Method::POST, "/pair", Some(body)).await?;
    if resp.status().as_u16() == 429 {
        return Err(Error::Other("too many pairings started; wait a minute".into()));
    }
    data(resp).await
}

pub async fn pair_poll(address: &str, pairing: &Pairing) -> Result<Option<PairToken>> {
    let body = serde_json::json!({ "code": pairing.code, "secret": pairing.secret });
    let resp = send(address, address, None, Method::POST, "/pair/token", Some(body)).await?;
    match resp.status().as_u16() {
        202 => Ok(None),
        404 => Err(Error::Other("the link was refused or the code expired".into())),
        _ => data(resp).await.map(Some),
    }
}

pub async fn servers(panel: &Panel) -> Result<Vec<Server>> {
    data(call(panel, Method::GET, "/servers", None).await?).await
}

pub async fn server(panel: &Panel, slug: &str) -> Result<Server> {
    check_id(slug)?;
    let resp = call(panel, Method::GET, &format!("/servers/{}", encode(slug)), None).await?;
    if resp.status().as_u16() == 404 {
        return Err(panel_error(&panel.name, format!("has no server {slug}")));
    }
    data(resp).await
}

pub async fn client_mods(panel: &Panel, slug: &str) -> Result<ClientMods> {
    check_id(slug)?;
    let resp = call(panel, Method::GET, &format!("/servers/{}/client-mods", encode(slug)), None).await?;
    match resp.status().as_u16() {
        404 => Ok(ClientMods::default()),
        409 => Err(Error::NotReady("the server is still installing its pack".into())),
        _ => Ok(ok(resp)?.json().await?),
    }
}

pub fn client_mod_url(panel: &Panel, slug: &str, file: &str) -> Result<String> {
    check_id(slug)?;
    if matches!(file, "" | "." | "..") || file.contains(['/', '\\']) {
        return Err(Error::Other(format!("{file:?} is not a plain file name")));
    }
    Ok(format!("{}/api/v1/servers/{}/client-mods/{}", panel.address, encode(slug), encode(file)))
}

pub fn panel_file(panel: &Panel, slug: &str, file: &str) -> Result<Origin> {
    Ok(Origin::PanelFile { url: client_mod_url(panel, slug, file)?, bearer: token(panel)? })
}

fn no_key(panel: &Panel) -> Error {
    Error::Other(format!("the panel {} has no CurseForge key", panel.name))
}

pub async fn client_pack(panel: &Panel, slug: &str) -> Result<ClientPack> {
    check_id(slug)?;
    let resp = call(panel, Method::GET, &format!("/servers/{}/client-pack", encode(slug)), None).await?;
    if resp.status().as_u16() == 409 {
        return Err(no_key(panel));
    }
    data(resp).await
}

pub async fn client_pack_files(panel: &Panel, slug: &str, files: &[u64]) -> Result<Vec<PackFile>> {
    check_id(slug)?;
    let path = format!("/servers/{}/client-pack/files", encode(slug));
    let mut out = Vec::new();
    for chunk in files.chunks(2000) {
        let body = serde_json::json!({ "files": chunk });
        let resp = call(panel, Method::POST, &path, Some(body)).await?;
        if resp.status().as_u16() == 409 {
            return Err(no_key(panel));
        }
        out.extend(data::<Vec<PackFile>>(resp).await?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::tmp_dir;

    #[test]
    fn a_picture_is_a_small_png_or_a_known_logo() {
        let ok = |s: &str| picture(s).is_some();
        assert!(ok("data:image/png;base64,iVBORw0KGgo="));
        assert!(ok("https://cdn.modrinth.com/data/x/icon.png"));
        assert!(ok("https://media.forgecdn.net/avatars/1/2.png"));
        assert!(ok("https://cdn.feed-the-beast.com/a/b.webp"));
        for bad in [
            "javascript:alert(1)",
            "http://cdn.modrinth.com/a.png",
            "https://example.org/a.png",
            "https://cdn.modrinth.com.evil.example/a.png",
            "data:image/jpeg;base64,AAAA",
            "data:image/png;base64,AA\"AA",
            "data:image/png;base64,AA AA",
            "https://cdn.modrinth.com/a\"b.png",
            "https://cdn.modrinth.com/a b.png",
            "https://cdn.modrinth.com/a<b.png",
            "https://cdn.modrinth.com/a\\b.png",
            "https://cdn.modrinth.com/\u{e9}.png",
            "",
        ] {
            assert!(!ok(bad), "{bad}");
        }
        assert!(!ok(&format!("data:image/png;base64,{}", "A".repeat(200_000))));
        assert!(ok(&format!("data:image/png;base64,{}", "A".repeat(200_000 - 22))));
        assert!(!ok(&format!("https://cdn.modrinth.com/{}", "a".repeat(300))));
    }

    #[test]
    fn a_server_with_a_bad_picture_loads_with_none() {
        let json = |p: &str| format!(r#"{{"slug":"s","name":"S","hostname":"h","state":"running","game_version":null,"modpack":null,"loader":null,"players":null{p}}}"#);
        let get = |p: &str| serde_json::from_str::<Server>(&json(p)).unwrap().picture;
        assert_eq!(get(""), None);
        assert_eq!(get(r#","picture":null"#), None);
        assert_eq!(get(r#","picture":"javascript:alert(1)""#), None);
        assert_eq!(get(r#","picture":"https://cdn.modrinth.com/a.png""#).as_deref(), Some("https://cdn.modrinth.com/a.png"));
    }

    #[test]
    fn who_is_online_and_why_load_cut_to_size() {
        let json = |extra: &str| format!(r#"{{"slug":"s","name":"S","hostname":"h","state":"running","game_version":null,"modpack":null,"loader":null{extra}}}"#);
        let old: Server = serde_json::from_str(&json(r#","players":{"online":1,"max":5}"#)).unwrap();
        assert!(old.players.unwrap().names.is_empty() && old.detail.is_none());
        let none: Server = serde_json::from_str(&json(r#","players":{"online":1,"max":5,"names":null}"#)).unwrap();
        assert!(none.players.unwrap().names.is_empty());
        let odd: Server = serde_json::from_str(&json(r#","players":{"online":2,"max":5,"names":["Alex",7]}"#)).unwrap();
        assert_eq!(odd.players.unwrap().names, ["Alex"]);
        let long = "é".repeat(100);
        let many: Vec<String> = (0..250).map(|_| long.clone()).collect();
        let body = json(&format!(r#","players":{{"online":250,"max":300,"names":{}}},"detail":"{}""#, serde_json::to_string(&many).unwrap(), "ü".repeat(400)));
        let s: Server = serde_json::from_str(&body).unwrap();
        let names = s.players.unwrap().names;
        assert_eq!(names.len(), 200);
        assert!(names.iter().all(|n| n.chars().count() == 64));
        assert_eq!(s.detail.unwrap().chars().count(), 300);
    }

    #[test]
    fn the_host_keeps_its_port() {
        assert_eq!(host("https://panel.example.org:8443/sub"), "panel.example.org:8443");
        assert_eq!(host("http://10.0.0.5"), "10.0.0.5");
    }

    #[test]
    fn addresses() {
        assert_eq!(normalize_address(" panel.example.org/ ", false).unwrap(), "https://panel.example.org");
        assert_eq!(normalize_address("https://panel.example.org/api/v1/", false).unwrap(), "https://panel.example.org");
        assert!(normalize_address("http://10.0.0.5:8080", false).is_err());
        assert_eq!(normalize_address("http://10.0.0.5:8080", true).unwrap(), "http://10.0.0.5:8080");
        assert!(normalize_address("ftp://x", true).is_err());
        for bad in ["https://user:pw@panel.example.org", "https://panel.example.org/?x=1", "https://panel.example.org/#f", "https://panel.example.org/a?b", "https://"] {
            assert!(normalize_address(bad, true).is_err(), "{bad}");
        }
        assert_eq!(normalize_address("HTTPS://Panel.Example.org/API/V1/", false).unwrap(), "https://panel.example.org");
        assert_eq!(normalize_address("https://panel.example.org:8443/sub/", false).unwrap(), "https://panel.example.org:8443/sub");
        assert_eq!(normalize_address("https://panel.example.org/Sub/api/v1", false).unwrap(), "https://panel.example.org/Sub");
    }

    fn unique_host() -> String {
        format!("{}.example.org", &secrets::test_account()[5..].replace(':', "-"))
    }

    #[test]
    fn panels_file_has_no_token() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let address = format!("https://{}", unique_host());
        let p = add(&paths, &address, "Home", "tok-123", false).unwrap();
        assert!(!std::fs::read_to_string(paths.panels_file()).unwrap().contains("tok-123"));
        assert_eq!(token(&p).unwrap(), "tok-123");
        add(&paths, &format!("{address}/"), "Home again", "tok-456", false).unwrap();
        assert_eq!(list(&paths).unwrap().len(), 1);
        remove(&paths, &p.id).unwrap();
        assert!(token(&p).is_err() && list(&paths).unwrap().is_empty());
    }

    #[test]
    fn a_taken_id_is_numbered_and_the_id_is_the_host_only() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let host = unique_host();
        let a = add(&paths, &format!("https://{}/", host.to_uppercase()), "A", "t", false).unwrap();
        assert_eq!(a.id, host.replace('.', "-"));
        assert_eq!(a.address, format!("https://{host}"));
        let b = add(&paths, &format!("https://{host}:8443/sub/"), "B", "t", false).unwrap();
        assert_eq!(b.id, format!("{}-2", a.id));
        assert_eq!(b.address, format!("https://{host}:8443/sub"));
    }

    #[test]
    fn add_enforces_https() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let address = format!("http://{}", unique_host());
        assert!(add(&paths, &address, "A", "t", false).is_err());
        assert!(list(&paths).unwrap().is_empty());
        assert!(add(&paths, &address, "A", "t", true).unwrap().address.starts_with("http://"));
    }

    #[test]
    fn a_pairing_secret_is_never_serialized() {
        let p = Pairing { code: "ABCD".into(), secret: "pair-SECRET".into(), expires_at: "t".into(), interval: 1, approve_path: None };
        assert!(!serde_json::to_string(&p).unwrap().contains("pair-SECRET"));
        assert!(!format!("{p:?}").contains("pair-SECRET"));
    }

    #[test]
    fn a_mod_url_refuses_what_could_climb_out() {
        let panel = Panel { id: "x".into(), address: "https://panel.example.org".into(), name: "Home".into() };
        for (slug, file) in [("atm10", ""), ("atm10", "."), ("atm10", ".."), ("atm10", "a/b"), ("atm10", "a\\b"), ("..", "a.jar"), ("a/b", "a.jar"), ("", "a.jar")] {
            assert!(client_mod_url(&panel, slug, file).is_err(), "{slug} {file}");
        }
    }

    #[test]
    fn a_panel_file_is_built_from_the_panel() {
        let l = link("https://panel.example.org", "tok-SECRET-1");
        let Origin::PanelFile { url, bearer } = panel_file(&l.panel, "atm10", "a b.jar").unwrap() else { panic!() };
        assert_eq!(url, "https://panel.example.org/api/v1/servers/atm10/client-mods/a%20b.jar");
        assert_eq!(bearer, "tok-SECRET-1");
        assert!(panel_file(&l.panel, "atm10", "..").is_err());
    }

    use crate::download::testing::{serve, serve_full};
    use crate::sync::Origin;
    use std::sync::atomic::Ordering;

    const SERVERS: &str = include_str!("../tests/fixtures/panel-servers.json");
    const MODS: &str = include_str!("../tests/fixtures/panel-client-mods.json");

    struct Linked {
        panel: Panel,
    }

    fn link(address: &str, token: &str) -> Linked {
        let id = secrets::test_account().replace(':', "-");
        secrets::set(&account(&id), token).unwrap();
        Linked { panel: Panel { id, address: address.into(), name: "Home".into() } }
    }

    fn dead_address() -> String {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", closed.local_addr().unwrap())
    }

    #[test]
    fn parses_servers() {
        let v: serde_json::Value = serde_json::from_str(SERVERS).unwrap();
        let list: Vec<Server> = serde_json::from_value(v["data"].clone()).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].modpack.as_ref().unwrap().catalogue, "ftb");
        assert_eq!(list[0].loader.as_ref().unwrap().name, "neoforge");
        assert_eq!(list[0].players.as_ref().unwrap().max, 20);
        assert_eq!(list[1].loader.as_ref().unwrap().name, "fabric");
        assert!(list[2].modpack.is_none() && list[2].loader.is_none());
        assert_eq!(list[2].game_version.as_deref(), Some("1.21.10"));
    }

    #[test]
    fn parses_client_mods() {
        let m: ClientMods = serde_json::from_str(MODS).unwrap();
        assert_eq!(m.mods.len(), 2);
        assert!(m.mods[0].url.is_some() && m.mods[1].url.is_none());
        assert_eq!(m.removed, ["voicechat-neoforge-2.5.jar"]);
    }

    #[test]
    fn abilities() {
        assert_eq!(missing_abilities(&["servers:read".into()]), vec!["servers:join"]);
        assert!(missing_abilities(&["servers:read".into(), "servers:join".into(), "x".into()]).is_empty());
    }

    #[test]
    fn mod_urls_are_encoded() {
        let panel = Panel { id: "x".into(), address: "https://panel.example.org".into(), name: "Home".into() };
        assert_eq!(
            client_mod_url(&panel, "atm10", "my mod+1.jar").unwrap(),
            "https://panel.example.org/api/v1/servers/atm10/client-mods/my%20mod%2B1.jar"
        );
    }

    #[tokio::test]
    async fn servers_come_from_the_data_list() {
        let (url, _, heads) = serve_full(200, SERVERS.as_bytes().to_vec(), String::new()).await;
        let l = link(&url, "tok-1");
        let list = servers(&l.panel).await.unwrap();
        assert_eq!(list.len(), 3);
        let head = heads.lock().unwrap()[0].to_lowercase();
        assert!(head.starts_with("get /api/v1/servers "), "{head}");
        assert!(head.contains("authorization: bearer tok-1"));
    }

    #[tokio::test]
    async fn one_server_is_wrapped_in_data() {
        let one = br#"{"data":{"slug":"lobby","name":"Lobby","hostname":"lobby.mc.example.org","state":"offline"}}"#;
        let (url, _) = serve(200, one).await;
        let l = link(&url, "tok-1");
        let s = server(&l.panel, "lobby").await.unwrap();
        assert_eq!(s.slug, "lobby");
        assert!(server(&l.panel, "../x").await.is_err());
    }

    #[tokio::test]
    async fn a_missing_server_is_named() {
        let (url, _) = serve(404, b"").await;
        let l = link(&url, "tok-1");
        let err = server(&l.panel, "lobby").await.unwrap_err();
        assert!(matches!(err, Error::Panel { .. }));
        assert_eq!(err.to_string(), "panel Home: has no server lobby");
    }

    #[test]
    fn only_this_codes_own_path_is_opened() {
        let with = |code: &str, path: Option<&str>| Pairing { code: code.into(), secret: "s".into(), expires_at: "t".into(), interval: 1, approve_path: path.map(String::from) };
        let panel = "https://panel.example.org";
        assert_eq!(approve_page(panel, &with("7ABR547C", Some("/pair/7ABR547C"))), ("https://panel.example.org/pair/7ABR547C".into(), true));
        let tokens = ("https://panel.example.org/settings/api-tokens".to_string(), false);
        for path in [None, Some("/pair/OTHER123"), Some("//evil.org/pair/7ABR547C"), Some(".evil.org/pair/7ABR547C"), Some("https://evil.org/pair/7ABR547C"), Some("/pair/7ABR547C/../../x")] {
            assert_eq!(approve_page(panel, &with("7ABR547C", path)), tokens, "{path:?}");
        }
        assert_eq!(approve_page(panel, &with("../x", Some("/pair/../x"))), tokens);
    }

    #[tokio::test]
    async fn pending_pairing_is_none() {
        let (url, _) = serve(202, b"").await;
        let pairing = Pairing { code: "ABCD".into(), secret: "s".into(), expires_at: "t".into(), interval: 1, approve_path: None };
        assert!(pair_poll(&url, &pairing).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_pairing_starts_and_completes() {
        let body = br#"{"data":{"code":"ABCD","secret":"s","expires_at":"t","interval":2}}"#.to_vec();
        let (url, _, heads) = serve_full(201, body, String::new()).await;
        let p = pair_start(&url, &"n".repeat(80)).await.unwrap();
        assert_eq!((p.code.as_str(), p.interval, p.approve_path.as_deref()), ("ABCD", 2, None));
        let newer = br#"{"data":{"code":"ABCD","secret":"s","expires_at":"t","interval":2,"approve_path":"/pair/ABCD"}}"#;
        let (newer_url, _) = serve(201, newer).await;
        assert!(approve_page(&newer_url, &pair_start(&newer_url, "n").await.unwrap()).1);
        assert!(!heads.lock().unwrap()[0].to_lowercase().contains("authorization"));
        let (url, _) = serve(200, br#"{"data":{"token":"tok","abilities":["servers:read"],"user":{"name":"Jo"}}}"#).await;
        let t = pair_poll(&url, &p).await.unwrap().unwrap();
        assert_eq!((t.token.as_str(), t.user.name.as_deref()), ("tok", Some("Jo")));
        let (url, _) = serve(404, b"").await;
        assert!(pair_poll(&url, &p).await.err().unwrap().to_string().contains("expired"));
        let (url, _) = serve(429, b"").await;
        assert!(pair_start(&url, "n").await.unwrap_err().to_string().contains("too many"));
    }

    #[tokio::test]
    async fn a_revoked_token_says_link_again() {
        let (url, _) = serve(401, b"").await;
        let l = link(&url, "tok-SECRET-123");
        let err = servers(&l.panel).await.unwrap_err();
        assert!(matches!(err, Error::Panel { .. }));
        assert!(err.to_string().contains("link it again"));
    }

    #[tokio::test]
    async fn a_panel_that_wants_two_factor_says_so_itself() {
        let (url, _) = serve(403, br#"{"message":"This panel requires two-factor authentication. Set it up in the panel first."}"#).await;
        let l = link(&url, "tok-1");
        let msg = servers(&l.panel).await.unwrap_err().to_string();
        assert!(msg.contains("Set it up in the panel first") && !msg.contains("servers:join"), "{msg}");
    }

    #[tokio::test]
    async fn a_missing_ability_names_the_page() {
        let (url, _) = serve(403, b"").await;
        let l = link(&url, "tok-1");
        let msg = servers(&l.panel).await.unwrap_err().to_string();
        assert!(msg.contains("servers:join") && msg.contains(&format!("{url}/settings/api-tokens")), "{msg}");
        let (url, _) = serve(502, b"").await;
        let l = link(&url, "tok-1");
        assert!(servers(&l.panel).await.unwrap_err().to_string().contains("host service"));
    }

    #[tokio::test]
    async fn no_error_shows_the_token() {
        let token = "tok-SECRET-123";
        let (unauthorized, _) = serve(401, b"").await;
        let (broken, _) = serve(500, b"").await;
        let (junk, _) = serve(200, b"not json").await;
        let dead = dead_address();
        for address in [unauthorized, dead.clone(), broken, junk] {
            let l = link(&address, token);
            let err = servers(&l.panel).await.unwrap_err();
            for text in [err.to_string(), format!("{err:?}")] {
                assert!(!text.contains(token), "{text}");
            }
        }
        let l = link(&dead, token);
        let err = servers(&l.panel).await.unwrap_err();
        assert!(err.to_string().contains("cannot be reached"));
    }

    #[tokio::test]
    async fn a_redirect_never_carries_the_token_elsewhere() {
        let (other, other_hits) = serve(200, b"{}").await;
        let (url, _, _) = serve_full(302, vec![], format!("Location: {other}/stolen\r\n")).await;
        let l = link(&url, "tok-1");
        let err = servers(&l.panel).await.unwrap_err();
        assert!(err.to_string().contains("redirect"), "{err}");
        assert_eq!(other_hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_mods_is_empty_and_409_is_not_ready() {
        let (url, _) = serve(404, b"").await;
        let l = link(&url, "tok-1");
        let m = client_mods(&l.panel, "atm10").await.unwrap();
        assert!(m.mods.is_empty() && m.removed.is_empty());
        let (url, _) = serve(409, b"").await;
        let l = link(&url, "tok-1");
        let err = client_mods(&l.panel, "atm10").await.unwrap_err();
        assert!(matches!(err, Error::NotReady(_)));
        let (url, _) = serve(200, MODS.as_bytes()).await;
        let l = link(&url, "tok-1");
        assert_eq!(client_mods(&l.panel, "atm10").await.unwrap().mods.len(), 2);
    }

    #[tokio::test]
    async fn the_pack_needs_a_curseforge_key() {
        let (url, _) = serve(409, b"").await;
        let l = link(&url, "tok-1");
        let err = client_pack(&l.panel, "atm10").await.unwrap_err();
        assert_eq!(err.to_string(), "the panel Home has no CurseForge key");
        let (url, _) = serve(200, br#"{"data":{"project":1,"file":2,"archive":null,"page":"https://x/y"}}"#).await;
        let l = link(&url, "tok-1");
        let p = client_pack(&l.panel, "atm10").await.unwrap();
        assert_eq!((p.project, p.file, p.archive), (1, 2, None));
    }

    #[tokio::test]
    async fn file_ids_go_in_chunks_of_2000() {
        let (url, hits) = serve(200, br#"{"data":[{"project":1,"file":2,"name":"a","url":null,"page":"p"}]}"#).await;
        let l = link(&url, "tok-1");
        let ids: Vec<u64> = (0..4500).collect();
        let files = client_pack_files(&l.panel, "atm10", &ids).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        assert_eq!(files.len(), 3);
    }

    #[tokio::test]
    async fn no_request_follows_a_redirect() {
        for status in [302, 307, 308] {
            let (other, other_hits) = serve(200, b"{}").await;
            let (url, _, _) = serve_full(status, vec![], format!("Location: {other}/x\r\n")).await;
            let l = link(&url, "tok-1");
            let pairing = Pairing { code: "ABCD".into(), secret: "s".into(), expires_at: "t".into(), interval: 1, approve_path: None };
            assert!(servers(&l.panel).await.is_err());
            assert!(pair_poll(&url, &pairing).await.is_err());
            assert!(pair_start(&url, "n").await.is_err());
            assert_eq!(other_hits.load(Ordering::SeqCst), 0, "{status}");
        }
    }

    #[test]
    fn device_names() {
        assert_eq!(device_name(Some("DESKTOP-1".into())), "Spawnpoint Portal on DESKTOP-1");
        assert_eq!(device_name(None), if cfg!(windows) { "Spawnpoint Portal on this PC" } else { "Spawnpoint Portal on this computer" });
        assert!(computer_name().is_some_and(|n| n == n.trim() && !n.is_empty()));
        assert_eq!(device_name(Some("x".repeat(100))).chars().count(), 60);
    }
}
