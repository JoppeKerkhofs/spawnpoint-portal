use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::launch::Profile;
use crate::{Error, Paths, Result, panel_client, secrets, settings};

// The Azure app Mojang approved; a public client, so there is no secret.
pub const CLIENT_ID: &str = "21d26fc8-79b7-4546-8bc0-4683ab66831b";
const SCOPE: &str = "XboxLive.signin offline_access";

struct Hosts {
    microsoft: String,
    xbox: String,
    xsts: String,
    minecraft: String,
    account: String,
}

impl Hosts {
    fn real() -> Hosts {
        Hosts {
            microsoft: "https://login.microsoftonline.com".into(),
            xbox: "https://user.auth.xboxlive.com".into(),
            xsts: "https://xsts.auth.xboxlive.com".into(),
            minecraft: "https://api.minecraftservices.com".into(),
            account: "microsoft".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    #[serde(skip_serializing)]
    pub device_code: String,
}

#[derive(Deserialize)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

pub enum Poll {
    Pending,
    SlowDown,
    Done(MsTokens),
}

struct Xbox {
    token: String,
    user_hash: String,
}

// serde's own message can quote the value it choked on, and that value can be a token.
fn parse<T: DeserializeOwned>(body: &str, who: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|_| Error::Other(format!("{who} gave an answer Portal does not understand")))
}

fn sign_in(text: &str) -> Error {
    Error::SignIn(text.into())
}

fn parse_token_answer(status: u16, body: &str) -> Result<Poll> {
    if status == 200 {
        return Ok(Poll::Done(parse(body, "Microsoft")?));
    }
    let answer: Value = serde_json::from_str(body).unwrap_or_default();
    match answer["error"].as_str() {
        Some("authorization_pending") => Ok(Poll::Pending),
        Some("slow_down") => Ok(Poll::SlowDown),
        Some("authorization_declined") => Err(sign_in("the sign-in was declined")),
        Some("expired_token") => Err(sign_in("the code expired; start again")),
        Some("invalid_grant") => Err(sign_in("the saved sign-in is no longer valid; sign in again")),
        // Never `error_description`: it can echo request data.
        Some(other) => Err(Error::Other(format!("Microsoft refused the sign-in ({other})"))),
        None => Err(Error::Status { url: "Microsoft".into(), status }),
    }
}

fn parse_xbox(status: u16, body: &str) -> Result<Xbox> {
    let answer: Value = serde_json::from_str(body).unwrap_or_default();
    match status {
        200 => match (answer["Token"].as_str(), answer["DisplayClaims"]["xui"][0]["uhs"].as_str()) {
            (Some(token), Some(hash)) => Ok(Xbox { token: token.into(), user_hash: hash.into() }),
            _ => Err(Error::Other("Xbox Live gave an answer Portal does not understand".into())),
        },
        401 => Err(match answer["XErr"].as_u64() {
            Some(2148916233) => sign_in("this Microsoft account has no Xbox profile; make one at xbox.com first"),
            Some(2148916235) => sign_in("Xbox Live is not available in this account's country"),
            Some(2148916236 | 2148916237) => sign_in("this account needs adult verification on the Xbox site"),
            Some(2148916238) => sign_in("this is a child account; an adult has to add it to a Microsoft family"),
            other => Error::SignIn(format!("Xbox Live refused the sign-in ({})", other.unwrap_or(0))),
        }),
        _ => Err(Error::Status { url: "Xbox Live".into(), status }),
    }
}

fn parse_profile(status: u16, body: &str, mc_token: &str) -> Result<Profile> {
    #[derive(Deserialize)]
    struct Answer {
        id: String,
        name: String,
    }
    match status {
        200 => {
            let a: Answer = parse(body, "Minecraft")?;
            Ok(Profile { name: a.name, uuid: a.id, access_token: mc_token.into(), user_type: "msa".into() })
        }
        // Game Pass players have no entitlement but do have a profile, so the profile is the ownership check.
        404 => Err(sign_in(
            "this Microsoft account has no Minecraft: Java Edition player; buy the game, or start the official launcher once to choose a name",
        )),
        _ => Err(Error::Status { url: "Minecraft".into(), status }),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skin {
    // The texture at Mojang, https only; none while the player has the default skin.
    pub url: Option<String>,
    pub slim: bool,
}

fn parse_skin(status: u16, body: &str) -> Result<Skin> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        skins: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        state: String,
        url: String,
        #[serde(default)]
        variant: String,
    }
    if status != 200 {
        return Err(Error::Status { url: "Minecraft".into(), status });
    }
    let active = parse::<Answer>(body, "Minecraft")?.skins.into_iter().find(|s| s.state == "ACTIVE");
    let url = active.as_ref().and_then(|s| s.url.split_once("://textures.minecraft.net/texture/")).map(|(_, id)| id.to_string());
    Ok(Skin {
        url: url.filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())).map(|id| format!("https://textures.minecraft.net/texture/{id}")),
        slim: active.is_some_and(|s| s.variant.eq_ignore_ascii_case("slim")),
    })
}

// A skin is a PNG of 64 by 64, or 64 by 32 in the old layout; Mojang refuses anything else.
fn check_skin(png: &[u8]) -> Result<()> {
    let size = |at: usize| png.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let is_png = png.starts_with(b"\x89PNG\r\n\x1a\n") && png.get(12..16) == Some(b"IHDR");
    match (is_png, size(16), size(20)) {
        (true, Some(64), Some(64 | 32)) if png.len() <= 1024 * 1024 => Ok(()),
        _ => Err(Error::Other("a skin is a PNG picture of 64 by 64 pixels".into())),
    }
}

async fn skin_of(hosts: &Hosts, paths: &Paths) -> Result<Skin> {
    let profile = current(hosts, paths).await?;
    let url = format!("{}/minecraft/profile", hosts.minecraft);
    let (status, body) = answer(panel_client().get(url).bearer_auth(&profile.access_token)).await?;
    parse_skin(status, &body)
}

async fn upload_skin(hosts: &Hosts, paths: &Paths, png: &[u8], slim: bool) -> Result<Skin> {
    check_skin(png)?;
    let profile = current(hosts, paths).await?;
    // reqwest's multipart sits behind a feature, and this body is two parts.
    const EDGE: &str = "portal-skin-7f3a91c2";
    let mut body = format!(
        "--{EDGE}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{}\r\n--{EDGE}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n",
        if slim { "slim" } else { "classic" }
    )
    .into_bytes();
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{EDGE}--\r\n").as_bytes());
    let request = panel_client()
        .post(format!("{}/minecraft/profile/skins", hosts.minecraft))
        .bearer_auth(&profile.access_token)
        .header("Content-Type", format!("multipart/form-data; boundary={EDGE}"))
        .body(body);
    let (status, body) = answer(request).await?;
    parse_skin(status, &body)
}

// reqwest's own `.form()` sits behind a feature that pulls in another crate.
fn form(fields: &[(&str, &str)]) -> String {
    let encode = |s: &str| -> String {
        s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    };
    fields.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect::<Vec<_>>().join("&")
}

async fn post_form(url: String, fields: &[(&str, &str)]) -> Result<(u16, String)> {
    answer(panel_client().post(url).header("Content-Type", "application/x-www-form-urlencoded").body(form(fields))).await
}

async fn answer(request: reqwest::RequestBuilder) -> Result<(u16, String)> {
    let response = request.send().await?;
    Ok((response.status().as_u16(), response.text().await?))
}

async fn token(hosts: &Hosts, fields: &[(&str, &str)]) -> Result<Poll> {
    let url = format!("{}/consumers/oauth2/v2.0/token", hosts.microsoft);
    let (status, body) = post_form(url, fields).await?;
    parse_token_answer(status, &body)
}

async fn refresh(hosts: &Hosts, refresh_token: &str) -> Result<MsTokens> {
    let form = [("grant_type", "refresh_token"), ("client_id", CLIENT_ID), ("refresh_token", refresh_token), ("scope", SCOPE)];
    match token(hosts, &form).await? {
        Poll::Done(tokens) => Ok(tokens),
        _ => Err(Error::Other("Microsoft gave an answer Portal does not understand".into())),
    }
}

async fn xbox(url: String, body: Value) -> Result<Xbox> {
    let (status, body) = answer(panel_client().post(url).json(&body)).await?;
    parse_xbox(status, &body)
}

async fn chain(hosts: &Hosts, ms_access_token: &str) -> Result<Profile> {
    let user = xbox(
        format!("{}/user/authenticate", hosts.xbox),
        json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access_token}") },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }),
    )
    .await?;
    let xsts = xbox(
        format!("{}/xsts/authorize", hosts.xsts),
        json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [user.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }),
    )
    .await?;

    #[derive(Deserialize)]
    struct Login {
        access_token: String,
    }
    let identity = json!({ "identityToken": format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token) });
    let login = format!("{}/authentication/login_with_xbox", hosts.minecraft);
    let mc_token = match answer(panel_client().post(login).json(&identity)).await? {
        (200, body) => parse::<Login>(&body, "Minecraft")?.access_token,
        (403, _) => return Err(Error::Other("Mojang has not approved this launcher yet".into())),
        (status, _) => return Err(Error::Status { url: "Minecraft".into(), status }),
    };
    let url = format!("{}/minecraft/profile", hosts.minecraft);
    let (status, body) = answer(panel_client().get(url).bearer_auth(&mc_token)).await?;
    parse_profile(status, &body, &mc_token)
}

fn remember(paths: &Paths, name: Option<&str>) -> Result<()> {
    let mut s = settings::load(paths);
    if s.account_name.as_deref() != name {
        s.account_name = name.map(String::from);
        settings::save(paths, &s)?;
    }
    Ok(())
}

async fn finish(hosts: &Hosts, paths: &Paths, tokens: MsTokens) -> Result<String> {
    let profile = chain(hosts, &tokens.access_token).await?;
    secrets::set(&hosts.account, &tokens.refresh_token)?;
    remember(paths, Some(&profile.name))?;
    Ok(profile.name)
}

async fn current(hosts: &Hosts, paths: &Paths) -> Result<Profile> {
    let Some(saved) = secrets::get(&hosts.account)? else {
        return Err(sign_in("sign in with Microsoft before starting a game"));
    };
    let tokens = match refresh(hosts, &saved).await {
        Err(e @ Error::SignIn(_)) => {
            forget(hosts, paths)?;
            return Err(e);
        }
        other => other?,
    };
    // Microsoft retires the old refresh token, so the new one is kept before anything else can fail.
    secrets::set(&hosts.account, &tokens.refresh_token)?;
    let profile = chain(hosts, &tokens.access_token).await?;
    remember(paths, Some(&profile.name))?;
    Ok(profile)
}

fn forget(hosts: &Hosts, paths: &Paths) -> Result<()> {
    secrets::delete(&hosts.account)?;
    remember(paths, None)
}

pub async fn device_code() -> Result<DeviceCode> {
    let url = format!("{}/consumers/oauth2/v2.0/devicecode", Hosts::real().microsoft);
    match post_form(url, &[("client_id", CLIENT_ID), ("scope", SCOPE)]).await? {
        (200, body) => parse(&body, "Microsoft"),
        (status, _) => Err(Error::Status { url: "Microsoft".into(), status }),
    }
}

pub async fn poll(code: &DeviceCode) -> Result<Poll> {
    let form = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", CLIENT_ID),
        ("device_code", code.device_code.as_str()),
    ];
    token(&Hosts::real(), &form).await
}

// Keeps the sign-in only when the chain ends in a player; returns the player name.
pub async fn finish_sign_in(paths: &Paths, tokens: MsTokens) -> Result<String> {
    finish(&Hosts::real(), paths, tokens).await
}

// The signed-in player; an error asking to sign in when there is none. Never an offline player.
pub async fn current_profile(paths: &Paths) -> Result<Profile> {
    current(&Hosts::real(), paths).await
}

pub async fn skin(paths: &Paths) -> Result<Skin> {
    skin_of(&Hosts::real(), paths).await
}

pub async fn set_skin(paths: &Paths, png: &[u8], slim: bool) -> Result<Skin> {
    upload_skin(&Hosts::real(), paths, png, slim).await
}

pub fn sign_out(paths: &Paths) -> Result<()> {
    forget(&Hosts::real(), paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::{Tmp, serve_routes, tmp_dir};

    const TOKEN: &[u8] = include_bytes!("../tests/fixtures/auth-token.json");
    const XBOX: &[u8] = include_bytes!("../tests/fixtures/auth-xbl.json");

    #[test]
    fn device_code_parses_and_hides_its_secret() {
        let d: DeviceCode = serde_json::from_str(include_str!("../tests/fixtures/auth-devicecode.json")).unwrap();
        assert!(d.verification_uri.starts_with("https://"));
        assert!(!serde_json::to_string(&d).unwrap().contains(&d.device_code));
    }

    #[test]
    fn forms_are_percent_encoded() {
        assert_eq!(form(&[("scope", SCOPE), ("t", "a+b/c=&d!")]), "scope=XboxLive.signin%20offline_access&t=a%2Bb%2Fc%3D%26d%21");
    }

    #[test]
    fn token_answers() {
        assert!(matches!(parse_token_answer(400, include_str!("../tests/fixtures/auth-token-pending.json")).unwrap(), Poll::Pending));
        assert!(matches!(parse_token_answer(400, r#"{"error":"slow_down"}"#).unwrap(), Poll::SlowDown));
        assert!(matches!(parse_token_answer(200, std::str::from_utf8(TOKEN).unwrap()).unwrap(), Poll::Done(t) if t.refresh_token == "REFRESH"));
        for e in ["authorization_declined", "expired_token", "invalid_grant"] {
            assert!(matches!(parse_token_answer(400, &format!(r#"{{"error":"{e}"}}"#)), Err(Error::SignIn(_))));
        }
        let other = parse_token_answer(400, r#"{"error":"invalid_client","error_description":"SECRET-ISH"}"#).err().unwrap().to_string();
        assert!(other.contains("invalid_client") && !other.contains("SECRET-ISH"));
        let broken = parse_token_answer(200, r#"{"access_token":"SECRET-ISH","refresh_token":5}"#).err().unwrap().to_string();
        assert!(!broken.contains("SECRET-ISH"));
        assert_eq!(parse_token_answer(503, "").err().unwrap().kind(), "network");
    }

    #[test]
    fn xbox_answers() {
        let x = parse_xbox(200, std::str::from_utf8(XBOX).unwrap()).unwrap();
        assert!(!x.user_hash.is_empty() && !x.token.is_empty());
        let e = parse_xbox(401, include_str!("../tests/fixtures/auth-xsts-error.json")).err().unwrap();
        assert!(matches!(e, Error::SignIn(_)));
        let text = |body: &str| parse_xbox(401, body).err().unwrap().to_string();
        assert!(text(r#"{"XErr":2148916238}"#).contains("child account"));
        assert!(text(r#"{"XErr":2148916233}"#).contains("Xbox profile"));
        assert!(text(r#"{"XErr":1}"#).contains("(1)"));
        assert!(!matches!(parse_xbox(500, ""), Err(Error::SignIn(_))));
    }

    #[test]
    fn profiles() {
        let p = parse_profile(200, include_str!("../tests/fixtures/auth-profile.json"), "MC").unwrap();
        assert_eq!((p.user_type.as_str(), p.access_token.as_str(), p.uuid.len()), ("msa", "MC", 32));
        assert!(matches!(parse_profile(404, "{}", "MC"), Err(Error::SignIn(_))));
        assert!(!matches!(parse_profile(500, "", "MC"), Err(Error::SignIn(_))));
    }

    fn hosts(base: &str) -> Hosts {
        Hosts { microsoft: base.into(), xbox: base.into(), xsts: base.into(), minecraft: base.into(), account: secrets::test_account() }
    }

    fn home() -> (Tmp, Paths) {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        (dir, paths)
    }

    fn signed_in(hosts: &Hosts, paths: &Paths) {
        secrets::set(&hosts.account, "REFRESH1").unwrap();
        remember(paths, Some("Old")).unwrap();
    }

    fn route(key: &'static str, status: u16, body: &[u8]) -> (&'static str, u16, Vec<u8>, String) {
        (key, status, body.to_vec(), String::new())
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut p = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        p.extend(width.to_be_bytes());
        p.extend(height.to_be_bytes());
        p
    }

    #[test]
    fn a_skin_is_read_from_the_profile_and_only_a_skin_sized_png_is_sent() {
        let body = r#"{"id":"0","name":"Steve","skins":[
            {"id":"a","state":"INACTIVE","url":"http://textures.minecraft.net/texture/old","variant":"CLASSIC"},
            {"id":"b","state":"ACTIVE","url":"http://textures.minecraft.net/texture/abc123","variant":"SLIM"}]}"#;
        assert_eq!(parse_skin(200, body).unwrap(), Skin { url: Some("https://textures.minecraft.net/texture/abc123".into()), slim: true });
        assert_eq!(parse_skin(200, include_str!("../tests/fixtures/auth-profile.json")).unwrap(), Skin { url: None, slim: false });
        let elsewhere = r#"{"skins":[{"state":"ACTIVE","url":"https://evil.example/texture/abc"}]}"#;
        assert_eq!(parse_skin(200, elsewhere).unwrap().url, None);
        assert!(parse_skin(401, "").is_err());

        assert!(check_skin(&png(64, 64)).is_ok() && check_skin(&png(64, 32)).is_ok());
        assert!(check_skin(&png(128, 128)).is_err() && check_skin(b"GIF89a").is_err() && check_skin(b"").is_err());
    }

    #[tokio::test]
    async fn a_new_skin_is_sent_with_the_game_token_and_its_kind() {
        let mut routes = vec![route("/minecraft/profile/skins", 200, br#"{"skins":[{"state":"ACTIVE","url":"http://textures.minecraft.net/texture/new1","variant":"SLIM"}]}"#)];
        routes.extend(whole_chain(200));
        let (base, _, heads) = serve_routes(routes).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        signed_in(&h, &paths);
        let skin = upload_skin(&h, &paths, &png(64, 64), true).await.unwrap();
        assert_eq!(skin, Skin { url: Some("https://textures.minecraft.net/texture/new1".into()), slim: true });
        let heads = heads.lock().unwrap();
        let sent = heads.last().unwrap().to_ascii_lowercase();
        assert!(sent.starts_with("post /minecraft/profile/skins") && sent.contains("authorization: bearer mc") && sent.contains("multipart/form-data; boundary="));

        assert!(upload_skin(&h, &paths, b"not a picture", false).await.is_err());
        assert_eq!(heads.len(), 6);
    }

    #[tokio::test]
    async fn signed_out_cannot_start_a_game() {
        let (base, hits, _) = serve_routes(vec![]).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        assert!(matches!(current(&h, &paths).await, Err(Error::SignIn(_))));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_dead_refresh_token_asks_to_sign_in_again() {
        let (base, _, _) = serve_routes(vec![route("/token", 400, br#"{"error":"invalid_grant"}"#)]).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        signed_in(&h, &paths);
        assert!(matches!(current(&h, &paths).await, Err(Error::SignIn(_))));
        assert_eq!(secrets::get(&h.account).unwrap(), None);
        assert_eq!(settings::load(&paths).account_name, None);
    }

    #[tokio::test]
    async fn a_network_error_keeps_the_sign_in() {
        let (h, (_dir, paths)) = (hosts("http://127.0.0.1:1"), home());
        signed_in(&h, &paths);
        assert!(matches!(current(&h, &paths).await, Err(Error::Http(_))));
        assert_eq!(secrets::get(&h.account).unwrap().as_deref(), Some("REFRESH1"));
        assert_eq!(settings::load(&paths).account_name.as_deref(), Some("Old"));
    }

    #[tokio::test]
    async fn a_new_refresh_token_replaces_the_old() {
        let (base, _, _) = serve_routes(vec![route("/token", 200, TOKEN), route("/user/authenticate", 500, b"")]).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        signed_in(&h, &paths);
        assert!(current(&h, &paths).await.is_err());
        assert_eq!(secrets::get(&h.account).unwrap().as_deref(), Some("REFRESH"));
    }

    fn whole_chain(profile_status: u16) -> Vec<(&'static str, u16, Vec<u8>, String)> {
        vec![
            route("/token", 200, TOKEN),
            route("/user/authenticate", 200, XBOX),
            route("/xsts/authorize", 200, XBOX),
            route("/login_with_xbox", 200, br#"{"access_token":"MC"}"#),
            route("/minecraft/profile", profile_status, include_bytes!("../tests/fixtures/auth-profile.json")),
        ]
    }

    #[tokio::test]
    async fn the_chain_ends_in_the_player_and_sends_each_token_one_step_on() {
        let (base, _, heads) = serve_routes(whole_chain(200)).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        signed_in(&h, &paths);
        let p = current(&h, &paths).await.unwrap();
        assert_eq!((p.access_token.as_str(), p.user_type.as_str()), ("MC", "msa"));
        assert_eq!(settings::load(&paths).account_name, Some(p.name));
        let heads = heads.lock().unwrap();
        assert_eq!(heads.len(), 5);
        assert!(heads[4].to_ascii_lowercase().contains("authorization: bearer mc"));
    }

    #[tokio::test]
    async fn a_sign_in_without_a_player_is_not_kept() {
        let (base, _, _) = serve_routes(whole_chain(404)).await;
        let (h, (_dir, paths)) = (hosts(&base), home());
        let tokens = MsTokens { access_token: "ACCESS".into(), refresh_token: "REFRESH".into() };
        assert!(matches!(finish(&h, &paths, tokens).await, Err(Error::SignIn(_))));
        assert_eq!(secrets::get(&h.account).unwrap(), None);
        assert_eq!(settings::load(&paths).account_name, None);
    }
}
