use reqwest::{Client, Method, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::instance::slug;
use crate::paths::check_id;
use crate::{Error, Paths, Result, secrets};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    pub address: String,
    pub name: String,
}

pub fn normalize_address(input: &str, allow_http: bool) -> Result<String> {
    let input = input.trim();
    let full = if input.contains("://") { input.to_string() } else { format!("https://{input}") };
    let lower = full.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(Error::Other(format!("{input} is not an http(s) address")));
    }
    let trimmed = full.trim_end_matches('/');
    let address = trimmed.strip_suffix("/api/v1").unwrap_or(trimmed).trim_end_matches('/').to_string();
    if lower.starts_with("http://") && !allow_http {
        return Err(Error::Other(format!("{address} is not https; pass --allow-http to link it anyway")));
    }
    Ok(address)
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

pub fn add(paths: &Paths, address: &str, name: &str, token: &str) -> Result<Panel> {
    let mut panels = list(paths)?;
    let panel = if let Some(old) = panels.iter_mut().find(|p| p.address == address) {
        old.name = name.to_string();
        old.clone()
    } else {
        let host = address.split("://").nth(1).unwrap_or(address);
        let base = slug(host.split('/').next().unwrap_or(host));
        let mut id = base.clone();
        let mut n = 2;
        while panels.iter().any(|p| p.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let panel = Panel { id, address: address.to_string(), name: name.to_string() };
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
    pub secret: String,
    pub expires_at: String,
    pub interval: u64,
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

// The token goes in the header only. `client` must not follow redirects (`panel_client`).
async fn send(
    client: &Client,
    name: &str,
    address: &str,
    token: Option<&str>,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    let mut req = client.request(method, format!("{address}/api/v1{path}"));
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    if let Some(body) = body {
        req = req.json(&body);
    }
    let resp = req.send().await.map_err(|_| panel_error(name, "cannot be reached"))?;
    let status = resp.status();
    match status.as_u16() {
        401 => Err(panel_error(name, "no longer accepts this launcher; link it again")),
        403 => Err(panel_error(
            name,
            format!("this launcher's token may not do that: it needs servers:read and servers:join, granted at {address}/settings/api-tokens"),
        )),
        502 => Err(panel_error(name, "the panel's host service is not answering")),
        _ if status.is_redirection() => Err(panel_error(name, "answered with a redirect; check the panel address")),
        _ => Ok(resp),
    }
}

async fn call(
    client: &Client,
    panel: &Panel,
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<Response> {
    let token = token(panel)?;
    send(client, &panel.name, &panel.address, Some(&token), method, path, body).await
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

pub async fn pair_start(client: &Client, address: &str, device_name: &str) -> Result<Pairing> {
    let name: String = device_name.chars().take(60).collect();
    let body = serde_json::json!({ "device_name": name });
    let resp = send(client, address, address, None, Method::POST, "/pair", Some(body)).await?;
    if resp.status().as_u16() == 429 {
        return Err(Error::Other("too many pairings started; wait a minute".into()));
    }
    data(resp).await
}

pub async fn pair_poll(client: &Client, address: &str, pairing: &Pairing) -> Result<Option<PairToken>> {
    let body = serde_json::json!({ "code": pairing.code, "secret": pairing.secret });
    let resp = send(client, address, address, None, Method::POST, "/pair/token", Some(body)).await?;
    match resp.status().as_u16() {
        202 => Ok(None),
        404 => Err(Error::Other("the pairing code expired or was already used".into())),
        _ => data(resp).await.map(Some),
    }
}

pub async fn servers(client: &Client, panel: &Panel) -> Result<Vec<Server>> {
    data(call(client, panel, Method::GET, "/servers", None).await?).await
}

pub async fn server(client: &Client, panel: &Panel, slug: &str) -> Result<Server> {
    check_id(slug)?;
    data(call(client, panel, Method::GET, &format!("/servers/{}", encode(slug)), None).await?).await
}

pub async fn client_mods(client: &Client, panel: &Panel, slug: &str) -> Result<ClientMods> {
    check_id(slug)?;
    let resp = call(client, panel, Method::GET, &format!("/servers/{}/client-mods", encode(slug)), None).await?;
    match resp.status().as_u16() {
        404 => Ok(ClientMods::default()),
        409 => Err(Error::NotReady("the server is still installing its pack".into())),
        _ => Ok(ok(resp)?.json().await?),
    }
}

pub fn client_mod_url(panel: &Panel, slug: &str, file: &str) -> String {
    format!("{}/api/v1/servers/{}/client-mods/{}", panel.address, encode(slug), encode(file))
}

fn no_key(panel: &Panel) -> Error {
    Error::Other(format!("the panel {} has no CurseForge key", panel.name))
}

pub async fn client_pack(client: &Client, panel: &Panel, slug: &str) -> Result<ClientPack> {
    check_id(slug)?;
    let resp = call(client, panel, Method::GET, &format!("/servers/{}/client-pack", encode(slug)), None).await?;
    if resp.status().as_u16() == 409 {
        return Err(no_key(panel));
    }
    data(resp).await
}

pub async fn client_pack_files(client: &Client, panel: &Panel, slug: &str, files: &[u64]) -> Result<Vec<PackFile>> {
    check_id(slug)?;
    let path = format!("/servers/{}/client-pack/files", encode(slug));
    let mut out = Vec::new();
    for chunk in files.chunks(2000) {
        let body = serde_json::json!({ "files": chunk });
        let resp = call(client, panel, Method::POST, &path, Some(body)).await?;
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
    fn addresses() {
        assert_eq!(normalize_address(" panel.example.org/ ", false).unwrap(), "https://panel.example.org");
        assert_eq!(normalize_address("https://panel.example.org/api/v1/", false).unwrap(), "https://panel.example.org");
        assert!(normalize_address("http://10.0.0.5:8080", false).is_err());
        assert_eq!(normalize_address("http://10.0.0.5:8080", true).unwrap(), "http://10.0.0.5:8080");
        assert!(normalize_address("ftp://x", true).is_err());
    }

    #[test]
    fn panels_file_has_no_token() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let host = format!("{}.example.org", &secrets::test_account()[5..].replace(':', "-"));
        let address = format!("https://{host}");
        let p = add(&paths, &address, "Home", "tok-123").unwrap();
        assert!(!std::fs::read_to_string(paths.panels_file()).unwrap().contains("tok-123"));
        assert_eq!(token(&p).unwrap(), "tok-123");
        add(&paths, &address, "Home again", "tok-456").unwrap();
        assert_eq!(list(&paths).unwrap().len(), 1);
        remove(&paths, &p.id).unwrap();
        assert!(token(&p).is_err() && list(&paths).unwrap().is_empty());
    }

    #[test]
    fn a_taken_id_is_numbered() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let host = format!("{}.example.org", &secrets::test_account()[5..].replace(':', "-"));
        let a = add(&paths, &format!("https://{host}"), "A", "t").unwrap();
        let b = add(&paths, &format!("http://{host}"), "B", "t").unwrap();
        assert_eq!(b.id, format!("{}-2", a.id));
        remove(&paths, &a.id).unwrap();
        remove(&paths, &b.id).unwrap();
    }

    use crate::download::testing::{serve, serve_full};
    use crate::panel_client;
    use std::sync::atomic::Ordering;

    const SERVERS: &str = include_str!("../tests/fixtures/panel-servers.json");
    const MODS: &str = include_str!("../tests/fixtures/panel-client-mods.json");

    struct Linked {
        panel: Panel,
    }

    impl Drop for Linked {
        fn drop(&mut self) {
            let _ = secrets::delete(&account(&self.panel.id));
        }
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
            client_mod_url(&panel, "atm10", "my mod+1.jar"),
            "https://panel.example.org/api/v1/servers/atm10/client-mods/my%20mod%2B1.jar"
        );
    }

    #[tokio::test]
    async fn servers_come_from_the_data_list() {
        let (url, _, heads) = serve_full(200, SERVERS.as_bytes().to_vec(), String::new()).await;
        let l = link(&url, "tok-1");
        let list = servers(&panel_client(), &l.panel).await.unwrap();
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
        let s = server(&panel_client(), &l.panel, "lobby").await.unwrap();
        assert_eq!(s.slug, "lobby");
        assert!(server(&panel_client(), &l.panel, "../x").await.is_err());
    }

    #[tokio::test]
    async fn pending_pairing_is_none() {
        let (url, _) = serve(202, b"").await;
        let pairing = Pairing { code: "ABCD".into(), secret: "s".into(), expires_at: "t".into(), interval: 1 };
        assert!(pair_poll(&panel_client(), &url, &pairing).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_pairing_starts_and_completes() {
        let body = br#"{"data":{"code":"ABCD","secret":"s","expires_at":"t","interval":2}}"#.to_vec();
        let (url, _, heads) = serve_full(201, body, String::new()).await;
        let p = pair_start(&panel_client(), &url, &"n".repeat(80)).await.unwrap();
        assert_eq!((p.code.as_str(), p.interval), ("ABCD", 2));
        assert!(!heads.lock().unwrap()[0].to_lowercase().contains("authorization"));
        let (url, _) = serve(200, br#"{"data":{"token":"tok","abilities":["servers:read"],"user":{"name":"Jo"}}}"#).await;
        let t = pair_poll(&panel_client(), &url, &p).await.unwrap().unwrap();
        assert_eq!((t.token.as_str(), t.user.name.as_deref()), ("tok", Some("Jo")));
        let (url, _) = serve(404, b"").await;
        assert!(pair_poll(&panel_client(), &url, &p).await.err().unwrap().to_string().contains("expired"));
        let (url, _) = serve(429, b"").await;
        assert!(pair_start(&panel_client(), &url, "n").await.unwrap_err().to_string().contains("too many"));
    }

    #[tokio::test]
    async fn a_revoked_token_says_link_again() {
        let (url, _) = serve(401, b"").await;
        let l = link(&url, "tok-SECRET-123");
        let err = servers(&panel_client(), &l.panel).await.unwrap_err();
        assert!(matches!(err, Error::Panel { .. }));
        assert!(err.to_string().contains("link it again"));
    }

    #[tokio::test]
    async fn a_missing_ability_names_the_page() {
        let (url, _) = serve(403, b"").await;
        let l = link(&url, "tok-1");
        let msg = servers(&panel_client(), &l.panel).await.unwrap_err().to_string();
        assert!(msg.contains("servers:join") && msg.contains(&format!("{url}/settings/api-tokens")), "{msg}");
        let (url, _) = serve(502, b"").await;
        let l = link(&url, "tok-1");
        assert!(servers(&panel_client(), &l.panel).await.unwrap_err().to_string().contains("host service"));
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
            let err = servers(&panel_client(), &l.panel).await.unwrap_err();
            for text in [err.to_string(), format!("{err:?}")] {
                assert!(!text.contains(token), "{text}");
            }
        }
        let l = link(&dead, token);
        let err = servers(&panel_client(), &l.panel).await.unwrap_err();
        assert!(err.to_string().contains("cannot be reached"));
    }

    #[tokio::test]
    async fn a_redirect_never_carries_the_token_elsewhere() {
        let (other, other_hits) = serve(200, b"{}").await;
        let (url, _, _) = serve_full(302, vec![], format!("Location: {other}/stolen\r\n")).await;
        let l = link(&url, "tok-1");
        let err = servers(&panel_client(), &l.panel).await.unwrap_err();
        assert!(err.to_string().contains("redirect"), "{err}");
        assert_eq!(other_hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_mods_is_empty_and_409_is_not_ready() {
        let (url, _) = serve(404, b"").await;
        let l = link(&url, "tok-1");
        let m = client_mods(&panel_client(), &l.panel, "atm10").await.unwrap();
        assert!(m.mods.is_empty() && m.removed.is_empty());
        let (url, _) = serve(409, b"").await;
        let l = link(&url, "tok-1");
        let err = client_mods(&panel_client(), &l.panel, "atm10").await.unwrap_err();
        assert!(matches!(err, Error::NotReady(_)));
        let (url, _) = serve(200, MODS.as_bytes()).await;
        let l = link(&url, "tok-1");
        assert_eq!(client_mods(&panel_client(), &l.panel, "atm10").await.unwrap().mods.len(), 2);
    }

    #[tokio::test]
    async fn the_pack_needs_a_curseforge_key() {
        let (url, _) = serve(409, b"").await;
        let l = link(&url, "tok-1");
        let err = client_pack(&panel_client(), &l.panel, "atm10").await.unwrap_err();
        assert_eq!(err.to_string(), "the panel Home has no CurseForge key");
        let (url, _) = serve(200, br#"{"data":{"project":1,"file":2,"archive":null,"page":"https://x/y"}}"#).await;
        let l = link(&url, "tok-1");
        let p = client_pack(&panel_client(), &l.panel, "atm10").await.unwrap();
        assert_eq!((p.project, p.file, p.archive), (1, 2, None));
    }

    #[tokio::test]
    async fn file_ids_go_in_chunks_of_2000() {
        let (url, hits) = serve(200, br#"{"data":[{"project":1,"file":2,"name":"a","url":null,"page":"p"}]}"#).await;
        let l = link(&url, "tok-1");
        let ids: Vec<u64> = (0..4500).collect();
        let files = client_pack_files(&panel_client(), &l.panel, "atm10", &ids).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        assert_eq!(files.len(), 3);
    }
}
