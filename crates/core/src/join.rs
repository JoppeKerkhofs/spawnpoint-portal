use reqwest::Client;
use tokio::task::JoinSet;

use crate::download::{Hash, ProgressTx};
use crate::instance::{self, Instance, PackRef, Source};
use crate::launch::{DEFAULT_MEMORY_MB, Profile};
use crate::loader::{self, Loader, LoaderKind};
use crate::pack::{self, PackInstall, curseforge};
use crate::panel::{self, ClientMods, Panel, Server};
use crate::paths::check_id;
use crate::sync::{Manual, Origin, Wanted, safe_path};
use crate::{Error, Paths, Result, play};

fn find_instance(paths: &Paths, panel_id: &str, slug: &str) -> Result<Option<Instance>> {
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

async fn install(
    client: &Client,
    paths: &Paths,
    panel: &Panel,
    server: &Server,
    instance: &mut Instance,
    progress: &ProgressTx,
) -> Result<Vec<Manual>> {
    let pack = match &server.modpack {
        None => None,
        Some(mp) => {
            let version = mp.version.clone().ok_or_else(|| Error::NotReady("the server is still installing its pack".into()))?;
            let pack_ref = PackRef { catalogue: mp.catalogue.clone(), project: mp.project.clone(), version };
            let resolved = if mp.catalogue == "curseforge" {
                curseforge::resolve(client, paths, panel, &server.slug).await?
            } else {
                pack::resolve(client, paths, &pack_ref).await?
            };
            Some((pack_ref, resolved))
        }
    };
    let mods = panel::client_mods(panel, &server.slug).await?;
    let (pack_ref, resolved) = match pack {
        Some((r, p)) => (Some(r), Some(p)),
        None => (None, None),
    };
    let mut wanted = desired(panel, server, resolved, mods)?;
    if let Some(l) = &wanted.loader {
        wanted.loader = Some(loader::resolve(client, &wanted.minecraft, l).await?);
    }
    instance.source = Source::Server { panel: panel.id.clone(), server: server.slug.clone(), pack: pack_ref };
    pack::install(client, paths, instance, wanted, progress).await
}

pub async fn sync(
    client: &Client,
    paths: &Paths,
    panel: &Panel,
    slug: &str,
    progress: &ProgressTx,
) -> Result<(Instance, Server, Vec<Manual>)> {
    let server = panel::server(panel, slug).await?;
    let mut instance = instance_for(paths, panel, &server)?;
    match install(client, paths, panel, &server, &mut instance, progress).await {
        Ok(manual) => Ok((instance, server, manual)),
        Err(e) => {
            // An empty version means nothing was written for this new instance.
            if instance.minecraft.is_empty() {
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
) -> Result<(std::process::Child, Vec<Manual>)> {
    let (instance, server, manual) = sync(client, paths, panel, slug, progress).await?;
    check_host(&server.hostname)?;
    let child = play::start_instance(client, paths, &instance, profile, Some(server.hostname), DEFAULT_MEMORY_MB, progress).await?;
    Ok((child, manual))
}

pub fn instance_of(paths: &Paths, panel: &Panel, slug: &str) -> Result<Instance> {
    find_instance(paths, &panel.id, slug)?.ok_or_else(|| Error::Other(format!("no instance for {}/{slug}", panel.id)))
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
}
