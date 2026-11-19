use portal_core::download::{Progress, ProgressTx};
use portal_core::gamefiles::{install, manifest, version_json};
use portal_core::instance::{self, Instance, PackRef, Source};
use portal_core::java;
use portal_core::launch::{Profile, DEFAULT_MEMORY_MB};
use portal_core::loader::{self, Loader};
use portal_core::pack::{self, PackInstall};
use portal_core::play::start_instance;
use portal_core::sync::Manual;
use portal_core::{http_client, Error, Paths, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const USAGE: &str = "usage:
  portal versions                          list the release versions, newest first
  portal files <version>                   download the game files of a version
  portal java <component>                  download a Java runtime, e.g. java-runtime-delta
  portal instance list                     id, name, version, loader and source of each instance
  portal instance create <name> <version> [--loader <fabric|neoforge|forge>[:<version>]]
  portal instance delete <id>
  portal instance mods <id>                name, size, pack or own
  portal instance add-mod <id> <jar>
  portal instance remove-mod <id> <file name>
  portal pack install ftb <pack> <version> [--name <name>]
  portal pack install modrinth <version id> [--name <name>]
  portal pack update <id> <version>        install another version of the instance's pack
  portal launch <id> [--name <player>] [--server <host>] [--memory <MB>]
                                           start the game offline, wait for it to exit";

async fn with_progress<T, F: std::future::Future<Output = Result<T>>>(work: impl FnOnce(ProgressTx) -> F) -> Result<T> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let printer = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            print!("\r{} {}/{}", p.stage, p.done, p.total);
            let _ = std::io::stdout().flush();
        }
        println!();
    });
    let result = work(tx).await;
    let _ = printer.await;
    result
}

fn options(rest: &[String], known: &[&str]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let mut it = rest.iter();
    while let Some(f) = it.next() {
        let bad = || Error::Other(format!("unknown or incomplete option: {f}"));
        let value = it.next().filter(|v| !v.starts_with("--")).ok_or_else(bad)?;
        if !known.contains(&f.as_str()) {
            return Err(bad());
        }
        out.insert(f.clone(), value.clone());
    }
    Ok(out)
}

fn launch_options(rest: &[String]) -> Result<(Profile, Option<String>, u32)> {
    let mut o = options(rest, &["--name", "--server", "--memory"])?;
    let memory = match o.remove("--memory") {
        Some(m) => m.parse().map_err(|_| Error::Other(format!("invalid --memory: {m}")))?,
        None => DEFAULT_MEMORY_MB,
    };
    let name = o.remove("--name").unwrap_or_else(|| "Player".into());
    Ok((Profile::offline(&name), o.remove("--server"), memory))
}

fn loader_option(rest: &[String]) -> Result<Option<Loader>> {
    options(rest, &["--loader"])?.get("--loader").map(|l| Loader::parse(l)).transpose()
}

fn pack_name(rest: &[String], pack: &PackRef) -> Result<String> {
    Ok(options(rest, &["--name"])?.remove("--name").unwrap_or_else(|| {
        [&pack.catalogue, &pack.project, &pack.version].into_iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" ")
    }))
}

fn pack_ref(catalogue: &str, project: &str, version: &str) -> PackRef {
    PackRef { catalogue: catalogue.into(), project: project.into(), version: version.into() }
}

fn print_manual(paths: &Paths, instance: &Instance, manual: &[Manual]) {
    let game = instance::game_dir(paths, &instance.id);
    for m in manual {
        let full = game.join(&m.path);
        println!("fetch by hand: {}  {}  -> {}", m.name, m.page, full.parent().unwrap_or(&game).display());
    }
}

async fn install_pack(client: &Client, paths: &Paths, instance: &mut Instance, resolved: PackInstall) -> Result<()> {
    let inst = &mut *instance;
    let manual = with_progress(|tx| async move { pack::install(client, paths, inst, resolved, &tx).await }).await?;
    print_manual(paths, instance, &manual);
    Ok(())
}

async fn pack_install(rest: &[String], pack: PackRef) -> Result<()> {
    let name = pack_name(rest, &pack)?;
    let (client, paths) = (http_client(), Paths::new()?);
    let resolved = pack::resolve(&client, &paths, &pack).await?;
    let mut instance = instance::create(&paths, &name, &resolved.minecraft, resolved.loader.clone(), Source::Pack { pack })?;
    println!("instance {}", instance.id);
    install_pack(&client, &paths, &mut instance, resolved).await
}

async fn run(args: &[String]) -> Result<()> {
    match args {
        [cmd] if cmd == "versions" => {
            for v in manifest(&http_client()).await?.versions.iter().filter(|v| v.kind == "release") {
                println!("{}", v.id);
            }
        }
        [cmd, id] if cmd == "files" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let version = version_json(&client, &paths, id).await?;
            with_progress(|tx| async move { install(&client, &paths, &version, &tx).await }).await?;
        }
        [cmd, component] if cmd == "java" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let javaw = with_progress(|tx| async move { java::ensure(&client, &paths, component, &tx).await }).await?;
            println!("{}", javaw.display());
        }
        [cmd, sub] if cmd == "instance" && sub == "list" => {
            for i in instance::list(&Paths::new()?)? {
                let loader = i.loader.as_ref().map_or("-".to_string(), |l| format!("{}:{}", l.kind.as_str(), l.version.as_deref().unwrap_or("?")));
                let source = match &i.source {
                    Source::Manual => "manual".to_string(),
                    Source::Pack { pack } => format!("pack {} {} {}", pack.catalogue, pack.project, pack.version),
                    Source::Server { panel, server, .. } => format!("server {panel} {server}"),
                };
                println!("{}  {}  {}  {}  {}", i.id, i.name, i.minecraft, loader, source);
            }
        }
        [cmd, sub, name, version, rest @ ..] if cmd == "instance" && sub == "create" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let loader = match loader_option(rest)? {
                Some(l) => Some(loader::resolve(&client, version, &l).await?),
                None => None,
            };
            println!("{}", instance::create(&paths, name, version, loader, Source::Manual)?.id);
        }
        [cmd, sub, id] if cmd == "instance" && sub == "delete" => instance::delete(&Paths::new()?, id)?,
        [cmd, sub, id] if cmd == "instance" && sub == "mods" => {
            let paths = Paths::new()?;
            for m in instance::mods(&paths, &instance::load(&paths, id)?)? {
                println!("{}  {}  {}", m.name, m.size, if m.managed { "pack" } else { "own" });
            }
        }
        [cmd, sub, id, jar] if cmd == "instance" && sub == "add-mod" => {
            let paths = Paths::new()?;
            instance::add_mod(&paths, &instance::load(&paths, id)?, Path::new(jar))?;
        }
        [cmd, sub, id, name] if cmd == "instance" && sub == "remove-mod" => {
            let paths = Paths::new()?;
            instance::remove_mod(&paths, &mut instance::load(&paths, id)?, name)?;
        }
        [cmd, sub, cat, project, version, rest @ ..] if cmd == "pack" && sub == "install" && cat == "ftb" => {
            pack_install(rest, pack_ref("ftb", project, version)).await?
        }
        [cmd, sub, cat, version, rest @ ..] if cmd == "pack" && sub == "install" && cat == "modrinth" => {
            pack_install(rest, pack_ref("modrinth", "", version)).await?
        }
        [cmd, sub, id, version] if cmd == "pack" && sub == "update" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let mut instance = instance::load(&paths, id)?;
            let Source::Pack { pack } = &instance.source else {
                return Err(Error::Other(format!("instance {id} was not installed from a pack")));
            };
            let pack = pack_ref(&pack.catalogue, &pack.project, version);
            let resolved = pack::resolve(&client, &paths, &pack).await?;
            install_pack(&client, &paths, &mut instance, resolved).await?;
            instance.source = Source::Pack { pack };
            instance::save(&paths, &instance)?;
        }
        [cmd, id, rest @ ..] if cmd == "launch" => {
            let (profile, server, memory) = launch_options(rest)?;
            let (client, paths) = (http_client(), Paths::new()?);
            let instance = instance::load(&paths, id)?;
            let mut child = with_progress(|tx| async move { start_instance(&client, &paths, &instance, profile, server, memory, &tx).await }).await?;
            println!("started, pid {}", child.id());
            let code = child.wait()?.code().unwrap_or(1);
            std::process::exit(code);
        }
        _ => {
            println!("{USAGE}");
            std::process::exit(1);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launch_option_parsing() {
        let (p, server, memory) = launch_options(&args(&["--name", "Bob", "--server", "mc.test", "--memory", "2048"])).unwrap();
        assert_eq!((p.name, server, memory), ("Bob".into(), Some("mc.test".into()), 2048));
        let (p, server, memory) = launch_options(&[]).unwrap();
        assert_eq!((p.name.as_str(), server, memory), ("Player", None, DEFAULT_MEMORY_MB));
        for bad in [&["--server"][..], &["--srever", "h"], &["--memory=8"], &["--name", "--server", "x"], &["--memory", "lots"], &["--dir", "x"], &["--loader", "fabric"]] {
            assert!(launch_options(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn instance_create_and_pack_install_options() {
        let l = loader_option(&args(&["--loader", "neoforge:21.1.1"])).unwrap().unwrap();
        assert_eq!((l.kind.as_str(), l.version.as_deref()), ("neoforge", Some("21.1.1")));
        assert!(loader_option(&[]).unwrap().is_none());
        assert!(loader_option(&args(&["--loader", "rift"])).is_err());
        assert!(loader_option(&args(&["--name", "x"])).is_err());
        let ftb = pack_ref("ftb", "126", "100596");
        assert_eq!(pack_name(&[], &ftb).unwrap(), "ftb 126 100596");
        assert_eq!(pack_name(&args(&["--name", "FTB test"]), &ftb).unwrap(), "FTB test");
        assert_eq!(pack_name(&[], &pack_ref("modrinth", "", "uEkz53eA")).unwrap(), "modrinth uEkz53eA");
        assert!(pack_name(&args(&["--name"]), &ftb).is_err());
        assert!(pack_name(&args(&["--loader", "fabric"]), &ftb).is_err());
    }
}
