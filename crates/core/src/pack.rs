use reqwest::Client;

use crate::download::ProgressTx;
use crate::instance::{self, Instance, PackRef};
use crate::loader::Loader;
use crate::sync::{self, Manual, Wanted};
use crate::{Error, Paths, Result};

pub mod ftb;
pub mod modrinth;

#[derive(Debug, Clone)]
pub struct PackInstall {
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub files: Vec<Wanted>,
}

// For anything from the caller that ends up in a URL or one of our own file names.
pub(crate) fn check_id(id: &str) -> Result<()> {
    if id.is_empty() || id.bytes().all(|b| b == b'.') || !id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        return Err(Error::Other(format!("{id:?} is not a valid pack id")));
    }
    Ok(())
}

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
    instance.minecraft = pack.minecraft;
    instance.loader = pack.loader;
    instance::save(paths, instance)?;
    let game = instance::game_dir(paths, &instance.id);
    let plan = sync::plan(&game, &instance.managed, pack.files)?;
    sync::apply(client, paths, instance, plan, progress).await
}

#[cfg(test)]
mod tests {
    use super::check_id;

    #[test]
    fn ids() {
        for ok in ["126", "uEkz53eA", "a-b_c.d", "1.2.3"] {
            assert!(check_id(ok).is_ok(), "{ok}");
        }
        for bad in ["", ".", "..", "...", "a/b", "1?x", "a b", "a:b"] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }
}
