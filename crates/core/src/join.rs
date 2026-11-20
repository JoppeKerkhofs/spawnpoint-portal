use reqwest::Client;
use tokio::task::JoinSet;

use crate::download::{Hash, ProgressTx, numbered};
use crate::instance::{self, Instance, PackRef, Source};
use crate::launch::Profile;
use crate::loader::{self, Loader, LoaderKind};
use crate::pack::{self, PackInstall, curseforge};
use crate::panel::{self, ClientMods, Panel, Server};
use crate::paths::check_id;
use crate::sync::{Manual, Origin, Wanted, safe_path};
use crate::{Error, Paths, Result, play};

pub fn find_instance(paths: &Paths, panel_id: &str, slug: &str) -> Result<Option<Instance>> {
    Ok(instance::list(paths)?.into_iter().find(|i| matches!(&i.source, Source::Server { panel, server, .. } if panel == panel_id && server == slug)))
}

pub fn instance_for(paths: &Paths, panel: &Panel, server: &Server) -> Result<Instance> {
    if let Some(found) = find_instance(paths, &panel.id, &server.slug)? {
        return Ok(found);
    }
    let source = Source::Server { panel: panel.id.clone(), server: server.slug.clone(), pack: None };
    instance::create(paths, &server.name, "", None, source)
}

// The hostname becomes a game argument: host only, no port, nothing that parses as an option.
pub fn check_host(host: &str) -> Result<()> {
    let plain = host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    if host.is_empty() || host.len() > 253 || !plain || host.starts_with(['-', '.']) {
        return Err(Error::Other(format!("the panel sent a server address that is not a host name: {host:?}")));
    }
    Ok(())
}

pub fn parse_target(arg: &str, panels: &[Panel]) -> Result<(Panel, String)> {
    let (id, slug) = match arg.split_once('/') {
        Some((id, slug)) => (Some(id), slug),
        None => (None, arg),
    };
    if slug.is_empty() || slug.contains('/') {
        return Err(Error::Other(format!("{arg:?} is not <panel id>/<server>")));
    }
    let panel = match (id, panels) {
        (Some(id), _) => panels.iter().find(|p| p.id == id).ok_or_else(|| Error::Other(format!("no linked panel {id}")))?,
        (None, [only]) => only,
        (None, []) => return Err(Error::Other("no panel is linked; run \"portal panel add <address>\"".into())),
        (None, _) => return Err(Error::Other(format!("{arg:?} needs a panel: <panel id>/<server>"))),
    };
    Ok((panel.clone(), slug.to_string()))
}

fn pack_ref_of(mp: &panel::Modpack, version: String) -> PackRef {
    PackRef { catalogue: mp.catalogue.clone(), project: mp.project.clone(), version, version_name: mp.version_name.clone() }
}

async fn prepare(client: &Client, paths: &Paths, panel: &Panel, server: &Server) -> Result<(PackInstall, Option<PackRef>)> {
    let pack = match &server.modpack {
        None => None,
        Some(mp) => {
            let version = mp.version.clone().ok_or_else(|| Error::NotReady("the server is still installing its pack".into()))?;
            let pack_ref = pack_ref_of(mp, version);
            let resolved = if mp.catalogue == "curseforge" {
                curseforge::resolve(client, paths, panel, &server.slug).await?
            } else {
                pack::resolve(client, paths, &pack_ref).await?
            };
            Some((pack_ref, resolved))
        }
    };
    let mods = panel::client_mods(panel, &server.slug).await?;
    let (pack_ref, resolved) = pack.unzip();
    let mut wanted = desired(panel, server, resolved, mods)?;
    if let Some(l) = &wanted.loader {
        wanted.loader = Some(loader::resolve(client, &wanted.minecraft, l).await?);
    }
    Ok((wanted, pack_ref))
}

pub async fn sync(
    client: &Client,
    paths: &Paths,
    panel: &Panel,
    slug: &str,
    progress: &ProgressTx,
) -> Result<(Instance, Server, Vec<Manual>)> {
    let (instance, server, manual, _) = sync_numbered(client, paths, panel, slug, false, progress).await?;
    Ok((instance, server, manual))
}

// Numbers the events once the loader is known; returns the numbered sender for the steps after the sync.
async fn sync_numbered(
    client: &Client,
    paths: &Paths,
    panel: &Panel,
    slug: &str,
    launching: bool,
    progress: &ProgressTx,
) -> Result<(Instance, Server, Vec<Manual>, ProgressTx)> {
    let server = panel::server(panel, slug).await?;
    let (wanted, pack_ref) = prepare(client, paths, panel, &server).await?;
    let mut stages = play::SYNC_STAGES.to_vec();
    if launching {
        stages.extend(play::launch_stages(wanted.loader.as_ref().map(|l| l.kind)));
    }
    let progress = &numbered(stages, progress.clone());
    let existing = find_instance(paths, &panel.id, &server.slug)?;
    let created = existing.is_none();
    let mut instance = match existing {
        Some(i) => i,
        None => instance_for(paths, panel, &server)?,
    };
    instance.source = Source::Server { panel: panel.id.clone(), server: server.slug.clone(), pack: pack_ref };
    match pack::install(client, paths, &mut instance, wanted, progress).await {
        Ok(manual) => Ok((instance, server, manual, progress.clone())),
        Err(e) => {
            // An empty version means install failed before writing anything.
            if created && instance.minecraft.is_empty() && !matches!(e, Error::Busy(_)) {
                let _ = instance::delete(paths, &instance.id);
            }
            Err(e)
        }
    }
}

pub async fn join(
    client: &Client,
    paths: &Paths,
    panel: &Panel,
    slug: &str,
    profile: Profile,
    progress: &ProgressTx,
) -> Result<(std::process::Child, Instance, Vec<Manual>)> {
    let (instance, server, manual, progress) = sync_numbered(client, paths, panel, slug, true, progress).await?;
    check_host(&server.hostname)?;
    skip_onboarding(&instance::game_dir(paths, &instance.id))?;
    let child = play::start_instance(client, paths, &instance, profile, Some(server.hostname), instance.memory_mb(), &progress).await?;
    Ok((child, instance, manual))
}

// Minecraft shows an accessibility screen on a first start and only then runs quick play.
fn skip_onboarding(game: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(game)?;
    match std::fs::OpenOptions::new().write(true).create_new(true).open(game.join("options.txt")) {
        Ok(mut f) => Ok(std::io::Write::write_all(&mut f, b"onboardAccessibility:false\n")?),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub async fn all_servers(paths: &Paths) -> Result<Vec<(Panel, Result<Vec<Server>>)>> {
    let panels = panel::list(paths)?;
    let mut set = JoinSet::new();
    for (i, p) in panels.iter().cloned().enumerate() {
        set.spawn(async move { (i, panel::servers(&p).await) });
    }
    let mut results: Vec<Option<Result<Vec<Server>>>> = panels.iter().map(|_| None).collect();
    while let Some(done) = set.join_next().await {
        if let Ok((i, r)) = done {
            results[i] = Some(r);
        }
    }
    Ok(panels
        .into_iter()
        .zip(results)
        .map(|(p, r)| (p, r.unwrap_or_else(|| Err(Error::Other("the request was cancelled".into())))))
        .collect())
}

pub fn desired(panel: &Panel, server: &Server, pack: Option<PackInstall>, mods: ClientMods) -> Result<PackInstall> {
    build(server, pack, mods, |file| panel::panel_file(panel, &server.slug, file))
}

fn build(server: &Server, pack: Option<PackInstall>, mods: ClientMods, panel_file: impl Fn(&str) -> Result<Origin>) -> Result<PackInstall> {
    let minecraft = server
        .game_version
        .clone()
        .ok_or_else(|| Error::NotReady("the panel does not know this server's Minecraft version yet".into()))?;
    check_id(&minecraft)?;
    let (loader, mut files) = match pack {
        Some(p) => (p.loader, p.files),
        None => (server_loader(server)?, vec![]),
    };
    let removed: Vec<String> = mods.removed.iter().map(|n| format!("mods/{n}").to_lowercase()).collect();
    files.retain(|f| !removed.contains(&f.path.to_lowercase()));
    for m in mods.mods {
        if m.file.contains(['/', '\\']) {
            return Err(Error::Other(format!("{:?} is not a plain file name", m.file)));
        }
        let path = safe_path(&format!("mods/{}", m.file))?;
        let from = match m.url {
            Some(url) if url.starts_with("https://") => Origin::Url(url),
            Some(_) => return Err(Error::Other(format!("the panel lists a download that is not https: {}", m.file))),
            None => panel_file(&m.file)?,
        };
        files.retain(|f| f.path.to_lowercase() != path.to_lowercase());
        files.push(Wanted { path, from, hash: Some(Hash::Sha512(m.sha512)), size: Some(m.size) });
    }
    Ok(PackInstall { minecraft, loader, files })
}

fn server_loader(server: &Server) -> Result<Option<Loader>> {
    let Some(l) = &server.loader else { return Ok(None) };
    if let Some(v) = &l.version {
        check_id(v)?;
    }
    Ok(Some(Loader { kind: LoaderKind::parse(&l.name)?, version: l.version.clone() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::Hash;
    use crate::loader::LoaderKind;
    use crate::panel::{ClientMod, ServerLoader};

    fn server(game: Option<&str>, loader: Option<(&str, Option<&str>)>) -> Server {
        Server {
            slug: "atm10".into(),
            name: "ATM".into(),
            hostname: "atm10.mc.example.org".into(),
            state: "running".into(),
            game_version: game.map(Into::into),
            modpack: None,
            loader: loader.map(|(n, v)| ServerLoader { name: n.into(), version: v.map(Into::into) }),
            players: None,
        }
    }

    fn cm(file: &str, sha: &str, url: Option<&str>) -> ClientMod {
        ClientMod { file: file.into(), sha512: sha.into(), size: 1, url: url.map(Into::into) }
    }

    fn fake(file: &str) -> Result<Origin> {
        Ok(Origin::PanelFile { url: format!("https://p/{file}"), bearer: "BEARER".into() })
    }

    fn wanted(path: &str) -> Wanted {
        Wanted { path: path.into(), from: Origin::Url("https://x/y".into()), hash: Some(Hash::Sha1("1".into())), size: None }
    }

    fn run(s: &Server, pack: Option<PackInstall>, mods: ClientMods) -> Result<PackInstall> {
        build(s, pack, mods, fake)
    }

    #[test]
    fn an_instance_is_found_by_its_panel_and_server() {
        let dir = crate::download::testing::tmp_dir();
        let paths = Paths { root: dir.join("root") };
        assert!(find_instance(&paths, "home", "atm10").unwrap().is_none());
        let source = |p: &str, s: &str| Source::Server { panel: p.into(), server: s.into(), pack: None };
        instance::create(&paths, "Other", "1.21.1", None, source("home", "other")).unwrap();
        let made = instance::create(&paths, "ATM", "1.21.1", None, source("home", "atm10")).unwrap();
        assert_eq!(find_instance(&paths, "home", "atm10").unwrap().unwrap().id, made.id);
        assert!(find_instance(&paths, "away", "atm10").unwrap().is_none());
    }

    #[test]
    fn vanilla_server() {
        let d = run(&server(Some("1.21.10"), None), None, ClientMods::default()).unwrap();
        assert_eq!(d.minecraft, "1.21.10");
        assert!(d.loader.is_none() && d.files.is_empty());
    }

    #[test]
    fn plain_modded_server() {
        let mods = ClientMods { mods: vec![cm("a.jar", "aa", Some("https://cdn/a.jar")), cm("b.jar", "bb", None)], removed: vec![] };
        let d = run(&server(Some("1.21.1"), Some(("Fabric", None))), None, mods).unwrap();
        let l = d.loader.unwrap();
        assert_eq!((l.kind, l.version), (LoaderKind::Fabric, None));
        assert_eq!(d.files[0].path, "mods/a.jar");
        assert!(matches!(&d.files[0].from, Origin::Url(u) if u == "https://cdn/a.jar"));
        assert!(matches!(&d.files[0].hash, Some(Hash::Sha512(h)) if h == "aa"));
        assert!(!format!("{:?}", d.files[0]).contains("BEARER"));
        assert!(matches!(&d.files[1].from, Origin::PanelFile { url, bearer } if url == "https://p/b.jar" && bearer == "BEARER"));
    }

    #[test]
    fn pack_server_applies_the_servers_changes() {
        let pack = PackInstall {
            minecraft: "1.21.1".into(),
            loader: Some(Loader { kind: LoaderKind::NeoForge, version: Some("21.1.90".into()) }),
            files: vec![wanted("mods/x.jar"), wanted("mods/y.jar"), wanted("config/c.toml")],
        };
        let mods = ClientMods { mods: vec![cm("z.jar", "zz", None), cm("X.jar", "new", None)], removed: vec!["Y.JAR".into()] };
        let d = run(&server(Some("1.21.1"), Some(("neoforge", Some("1")))), Some(pack), mods).unwrap();
        let mut paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["config/c.toml", "mods/X.jar", "mods/z.jar"]);
        let x = d.files.iter().find(|f| f.path == "mods/X.jar").unwrap();
        assert!(matches!(&x.hash, Some(Hash::Sha512(h)) if h == "new"));
        assert_eq!(d.loader.unwrap().version.as_deref(), Some("21.1.90"));
    }

    #[test]
    fn unknown_version_is_not_ready() {
        let err = run(&server(None, None), None, ClientMods::default()).unwrap_err();
        assert!(matches!(err, Error::NotReady(_)));
    }

    #[test]
    fn odd_versions_and_loader_versions_are_refused() {
        for v in ["1.21/../x", "", "a b"] {
            assert!(run(&server(Some(v), None), None, ClientMods::default()).is_err(), "{v}");
        }
        assert!(run(&server(Some("1.21.1"), Some(("fabric", Some("1?x")))), None, ClientMods::default()).is_err());
    }

    #[test]
    fn a_mod_name_with_a_path_is_refused() {
        for file in [r"..\..\x.jar", "sub/x.jar", "..", "con.jar", "x.jar."] {
            let mods = ClientMods { mods: vec![cm(file, "a", None)], removed: vec![] };
            assert!(run(&server(Some("1.21.1"), None), None, mods).is_err(), "{file}");
        }
    }

    #[test]
    fn a_plain_http_mod_url_is_refused() {
        let mods = ClientMods { mods: vec![cm("a.jar", "a", Some("http://cdn/a.jar"))], removed: vec![] };
        let err = run(&server(Some("1.21.1"), None), None, mods).unwrap_err();
        assert!(err.to_string().contains("a.jar"));
    }

    fn panel(id: &str) -> Panel {
        Panel { id: id.into(), address: format!("https://{id}.example.org"), name: id.into() }
    }

    #[test]
    fn an_instance_belongs_to_a_panel_and_a_server() {
        let dir = crate::download::testing::tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let s = server(Some("1.21.1"), None);
        let a = instance_for(&paths, &panel("one"), &s).unwrap();
        assert_eq!(a.name, "ATM");
        assert_eq!(instance_for(&paths, &panel("one"), &s).unwrap().id, a.id);
        let b = instance_for(&paths, &panel("two"), &s).unwrap();
        assert_ne!(b.id, a.id);
        let mut other = server(Some("1.21.1"), None);
        other.slug = "lobby".into();
        assert_ne!(instance_for(&paths, &panel("one"), &other).unwrap().id, a.id);
        assert_eq!(crate::instance::list(&paths).unwrap().len(), 3);
    }

    #[test]
    fn hostnames() {
        for ok in ["mc.example.org", "10.0.0.5", "a-b.c", "localhost"] {
            assert!(check_host(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-demo", ".x", "a b", "a:25565", "a/b", "x;y", "a\nb", "ä.org", &"a".repeat(254)] {
            assert!(check_host(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn targets() {
        let one = [panel("home")];
        let two = [panel("home"), panel("work")];
        let t = |arg: &str, panels: &[Panel]| parse_target(arg, panels).map(|(p, s)| format!("{}/{s}", p.id));
        assert_eq!(t("home/atm10", &two).unwrap(), "home/atm10");
        assert_eq!(t("work/atm10", &two).unwrap(), "work/atm10");
        assert_eq!(t("atm10", &one).unwrap(), "home/atm10");
        assert_eq!(t("home/atm10", &one).unwrap(), "home/atm10");
        assert!(t("atm10", &two).is_err());
        assert!(t("atm10", &[]).is_err());
        assert!(t("gone/atm10", &two).is_err());
        assert!(t("home/", &two).is_err());
        assert!(t("home/a/b", &two).is_err());
    }

    #[test]
    fn quilt_server_is_unsupported() {
        let err = run(&server(Some("1.21.1"), Some(("quilt", None))), None, ClientMods::default()).unwrap_err();
        assert!(matches!(err, Error::Unsupported(_)));
    }

    use crate::download::testing::{serve_routes, tmp_dir};

    async fn test_panel(server_json: serde_json::Value, mods_status: u16) -> Panel {
        let body = serde_json::json!({ "data": server_json }).to_string().into_bytes();
        let routes = vec![("/client-mods", mods_status, vec![], String::new()), ("/servers/atm10", 200, body, String::new())];
        let (address, _, _) = serve_routes(routes).await;
        let id = crate::secrets::test_account().replace(':', "-");
        crate::secrets::set(&format!("panel:{id}"), "tok").unwrap();
        Panel { id, address, name: "Home".into() }
    }

    async fn failing_sync(paths: &Paths, panel: &Panel) -> Error {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        sync(&crate::http_client(), paths, panel, "atm10", &tx).await.unwrap_err()
    }

    fn atm10(modpack: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "slug": "atm10", "name": "ATM", "hostname": "atm10.mc.example.org", "state": "running", "game_version": "1.21.1", "modpack": modpack })
    }

    fn nothing_on_disk(paths: &Paths) -> bool {
        std::fs::read_dir(paths.instances()).map_or(true, |mut d| d.next().is_none())
    }

    #[tokio::test]
    async fn a_sync_that_cannot_start_creates_nothing() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let panel = test_panel(atm10(serde_json::Value::Null), 409).await;
        assert!(matches!(failing_sync(&paths, &panel).await, Error::NotReady(_)));
        assert!(nothing_on_disk(&paths));
    }

    #[tokio::test]
    async fn a_pack_without_a_version_creates_nothing() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let modpack = serde_json::json!({ "catalogue": "ftb", "project": "1", "version": null, "version_name": null });
        let panel = test_panel(atm10(modpack), 200).await;
        assert!(matches!(failing_sync(&paths, &panel).await, Error::NotReady(_)));
        assert!(nothing_on_disk(&paths));
    }

    #[tokio::test]
    async fn a_failed_sync_keeps_an_existing_instance_and_its_files() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let panel = test_panel(atm10(serde_json::Value::Null), 409).await;
        let inst = instance_for(&paths, &panel, &server(Some("1.21.1"), None)).unwrap();
        let own = instance::game_dir(&paths, &inst.id).join("mods/own.jar");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, b"mine").unwrap();
        assert!(matches!(failing_sync(&paths, &panel).await, Error::NotReady(_)));
        assert!(own.exists());
        assert_eq!(instance::list(&paths).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_dead_panel_does_not_hide_the_others() {
        let dir = tmp_dir();
        let paths = Paths { root: dir.join("home") };
        let list = serde_json::json!({ "data": [atm10(serde_json::Value::Null)] }).to_string().into_bytes();
        let (good, _, _) = serve_routes(vec![("/servers", 200, list, String::new())]).await;
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        panel::add(&paths, &good, "Good", "t", true).unwrap();
        panel::add(&paths, &dead, "Dead", "t", true).unwrap();
        let all = all_servers(&paths).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].1.as_ref().unwrap()[0].slug, "atm10");
        assert!(all[1].1.as_ref().unwrap_err().to_string().contains("cannot be reached"));
    }

    #[test]
    fn a_fresh_game_folder_skips_the_onboarding_but_options_are_kept() {
        let dir = tmp_dir();
        let game = dir.join("game");
        skip_onboarding(&game).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("options.txt")).unwrap().trim(), "onboardAccessibility:false");
        std::fs::write(game.join("options.txt"), "mine:1\n").unwrap();
        skip_onboarding(&game).unwrap();
        assert_eq!(std::fs::read_to_string(game.join("options.txt")).unwrap(), "mine:1\n");
    }

    #[test]
    fn the_pack_ref_carries_the_version_name() {
        let mp = panel::Modpack { catalogue: "ftb".into(), project: "1".into(), version: Some("9".into()), version_name: Some("1.3.7".into()) };
        let r = pack_ref_of(&mp, "9".into());
        assert_eq!((r.version.as_str(), r.version_name.as_deref()), ("9", Some("1.3.7")));
    }
}
