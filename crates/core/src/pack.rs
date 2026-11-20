use reqwest::Client;

use crate::download::{ProgressTx, blocking};
use crate::instance::{self, Instance, PackRef};
use crate::loader::Loader;
use crate::sync::{self, Manual, Wanted};
use crate::{Error, Paths, Result};

pub mod curseforge;
pub mod ftb;
pub mod modrinth;

#[derive(Debug, Clone)]
pub struct PackInstall {
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub files: Vec<Wanted>,
}

pub(crate) use crate::paths::check_id;

pub(crate) fn https_url(url: &str, name: &str) -> Result<String> {
    if !url.starts_with("https://") {
        return Err(Error::Other(format!("the pack lists a download that is not https: {name}")));
    }
    Ok(url.to_string())
}

pub async fn resolve(client: &Client, paths: &Paths, pack: &PackRef) -> Result<PackInstall> {
    match pack.catalogue.as_str() {
        "ftb" => ftb::resolve(client, &pack.project, &pack.version).await,
        "modrinth" => modrinth::resolve(client, paths, &pack.version).await,
        "curseforge" => Err(Error::Other("a CurseForge pack needs a panel's server".into())),
        other => Err(Error::Other(format!("unknown pack catalogue: {other}"))),
    }
}

pub async fn install(
    client: &Client,
    paths: &Paths,
    instance: &mut Instance,
    pack: PackInstall,
    progress: &ProgressTx,
) -> Result<Vec<Manual>> {
    if let Some(l) = &pack.loader {
        crate::loader::check_new(client, &pack.minecraft, l).await?;
    }
    let _lock = instance::lock(&instance.id)?;
    let game = instance::game_dir(paths, &instance.id);
    let (dir, managed, tx) = (game.clone(), instance.managed.clone(), progress.clone());
    let plan = blocking(move || sync::plan_with(&dir, &managed, pack.files, &tx)).await?;
    instance.minecraft = pack.minecraft;
    instance.loader = pack.loader;
    instance::save_synced(paths, instance)?;
    sync::apply(client, paths, instance, plan, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::testing::tmp_dir;
    use crate::instance::{ManagedFile, Source, create};
    use crate::sync::Origin;

    #[test]
    fn ids() {
        for ok in ["126", "uEkz53eA", "a-b_c.d", "1.2.3"] {
            assert!(check_id(ok).is_ok(), "{ok}");
        }
        for bad in ["", ".", "..", "...", "a/b", "1?x", "a b", "a:b"] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn a_refused_pack_leaves_the_instance_alone() {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        let mut inst = create(&paths, "P", "1.21.1", Some(Loader::parse("fabric:0.1").unwrap()), Source::Manual).unwrap();
        inst.managed.push(ManagedFile { path: "mods/a.jar".into(), sha1: "x".into() });
        instance::save(&paths, &inst).unwrap();
        let bad = Wanted { path: "../x".into(), from: Origin::Bytes(vec![]), hash: None, size: None };
        let pack = PackInstall { minecraft: "1.20.1".into(), loader: None, files: vec![bad] };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(install(&crate::http_client(), &paths, &mut inst, pack, &tx).await.is_err());
        let saved = instance::load(&paths, "p").unwrap();
        assert_eq!(saved.minecraft, "1.21.1");
        assert_eq!(saved, inst);
        assert_eq!(saved.loader, Some(Loader::parse("fabric:0.1").unwrap()));
        assert_eq!(saved.managed.len(), 1);
    }

    #[tokio::test]
    async fn a_forge_pack_is_checked_before_anything_is_written() {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        let mut inst = create(&paths, "F", "", None, Source::Manual).unwrap();
        let before = std::fs::read(paths.instances().join("f/instance.json")).unwrap();
        let forge = Some(Loader::parse("forge:7.8.1.738").unwrap());
        let pack = PackInstall { minecraft: "1.5.2".into(), loader: forge, files: vec![] };
        let dead = reqwest::Client::builder().proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap()).build().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(install(&dead, &paths, &mut inst, pack, &tx).await.is_err());
        assert_eq!((inst.minecraft.as_str(), inst.loader.clone()), ("", None));
        assert_eq!(std::fs::read(paths.instances().join("f/instance.json")).unwrap(), before);
    }

    #[tokio::test]
    async fn a_locked_instance_is_not_synced() {
        let t = tmp_dir();
        let paths = Paths { root: t.join("root") };
        let mut inst = create(&paths, "L", "1.21.1", None, Source::Manual).unwrap();
        let before = std::fs::read(paths.instances().join("l/instance.json")).unwrap();
        let _held = instance::lock(&inst.id).unwrap();
        let pack = PackInstall { minecraft: "1.20.1".into(), loader: None, files: vec![] };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let r = install(&crate::http_client(), &paths, &mut inst, pack, &tx).await;
        assert!(matches!(r, Err(Error::Busy(_))));
        assert_eq!(std::fs::read(paths.instances().join("l/instance.json")).unwrap(), before);
    }
}
