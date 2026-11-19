use crate::download::Hash;
use crate::loader::{Loader, LoaderKind};
use crate::pack::PackInstall;
use crate::panel::{self, ClientMods, Panel, Server};
use crate::paths::check_id;
use crate::sync::{Origin, Wanted, safe_path};
use crate::{Error, Result};

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

    #[test]
    fn quilt_server_is_unsupported() {
        let err = run(&server(Some("1.21.1"), Some(("quilt", None))), None, ClientMods::default()).unwrap_err();
        assert!(matches!(err, Error::Unsupported(_)));
    }
}
