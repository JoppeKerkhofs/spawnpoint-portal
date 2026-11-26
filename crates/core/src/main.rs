use portal_core::download::{numbered, Progress, ProgressTx};
use portal_core::auth::{self, Poll};
use portal_core::gamefiles::{install, manifest, version_json};
use portal_core::instance::{self, Instance, PackRef, Source};
use portal_core::java;
use portal_core::join;
use portal_core::panel;
use portal_core::loader::{self, Loader};
use portal_core::pack;
use portal_core::play::{launch_stages, start_instance};
use portal_core::sync::Manual;
use portal_core::{http_client, settings, Error, Paths, Result};
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
  portal panel add <address> [--allow-http]  link a Spawnpoint panel; approve the code it shows
  portal panel list                        id, name and address of each linked panel
  portal panel remove <id>                 unlink a panel and forget its token
  portal servers                           <panel id>/<server>, name, state, version, loader or pack
  portal sync <panel id>/<server>          make the server's instance match the server
  portal join <panel id>/<server>
                                           sync, then start the game on that server
                                           (the panel id may be left out when one panel is linked)
  portal launch <id> [--server <host>] [--memory <MB>]
                                           start the game, wait for it to exit
                                           (join and launch need a sign-in first)
  portal login                             sign in with Microsoft: enter the code it prints
  portal logout                            forget the Microsoft sign-in
  portal whoami                            the signed-in player";

async fn with_progress<T, F: std::future::Future<Output = Result<T>>>(work: impl FnOnce(ProgressTx) -> F) -> Result<T> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let printer = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            if p.steps > 0 {
                print!("\r\x1b[K{}/{} {} {}/{}", p.step, p.steps, p.stage, p.done, p.total);
            } else {
                print!("\r\x1b[K{} {}/{}", p.stage, p.done, p.total);
            }
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

async fn login() -> Result<()> {
    let code = auth::device_code().await?;
    println!("Go to {} and enter {}", show(&code.verification_uri), show(&code.user_code));
    let started = std::time::Instant::now();
    let mut interval = code.interval.max(1);
    let tokens = loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        match auth::poll(&code).await? {
            Poll::Done(tokens) => break tokens,
            Poll::SlowDown => interval += 5,
            Poll::Pending => {}
        }
        if started.elapsed().as_secs() > code.expires_in {
            return Err(Error::SignIn("the code expired; start again".into()));
        }
    };
    println!("signed in as {}", show(&auth::finish_sign_in(&Paths::new()?, tokens).await?));
    Ok(())
}

fn launch_options(rest: &[String]) -> Result<(Option<String>, Option<u32>)> {
    let mut o = options(rest, &["--server", "--memory"])?;
    let memory = o.remove("--memory").map(|m| m.parse().map_err(|_| Error::Other(format!("invalid --memory: {m}")))).transpose()?;
    Ok((o.remove("--server"), memory))
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
    PackRef { catalogue: catalogue.into(), project: project.into(), version: version.into(), version_name: None }
}

fn print_manual(paths: &Paths, instance: &Instance, manual: &[Manual]) {
    let game = instance::game_dir(paths, &instance.id);
    for m in manual {
        let full = game.join(&m.path);
        println!("fetch by hand: {}  {}  -> {}", show(&m.name), show(&m.page), full.parent().unwrap_or(&game).display());
    }
    for path in &instance.skipped {
        println!("left alone, your file is in the way: {}", show(path));
    }
}

// Text from a panel goes to the screen: no control characters, so no escape sequences.
fn show(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn allow_http(rest: &[String]) -> Result<bool> {
    match rest {
        [] => Ok(false),
        [f] if f == "--allow-http" => Ok(true),
        _ => Err(Error::Other(format!("unknown or incomplete option: {}", rest.join(" ")))),
    }
}

async fn panel_add(address: &str, allow_http: bool) -> Result<()> {
    let paths = Paths::new()?;
    let address = panel::normalize_address(address, allow_http)?;
    let pairing = panel::pair_start(&address, &panel::device_name(panel::computer_name())).await?;
    match panel::approve_page(&address, &pairing) {
        (page, true) => println!("Open {}, sign in and allow the code {}", show(&page), show(&pairing.code)),
        (page, false) => println!("Approve the code {} at {}", show(&pairing.code), show(&page)),
    }
    let started = std::time::Instant::now();
    let token = loop {
        tokio::time::sleep(std::time::Duration::from_secs(pairing.interval.max(1))).await;
        if let Some(t) = panel::pair_poll(&address, &pairing).await? {
            break t;
        }
        if started.elapsed() > std::time::Duration::from_secs(15 * 60) {
            return Err(Error::Other("nobody approved the code in time".into()));
        }
    };
    let linked = panel::add(&paths, &address, panel::host(&address), &token.token, allow_http)?;
    println!("linked {}", linked.name);
    let missing = panel::missing_abilities(&token.abilities);
    if !missing.is_empty() {
        eprintln!("warning: the token lacks {}; grant them at {address}/settings/api-tokens", missing.join(" and "));
    }
    Ok(())
}

async fn servers() -> Result<()> {
    for (p, result) in join::all_servers(&Paths::new()?).await? {
        match result {
            Err(e) => println!("{}: {}", p.id, show(&e.to_string())),
            Ok(list) => {
                for s in list {
                    let what = match (&s.modpack, &s.loader) {
                        (Some(m), _) => format!("pack {} {}", m.catalogue, m.version_name.as_deref().or(m.version.as_deref()).unwrap_or(&m.project)),
                        (None, Some(l)) => format!("{}:{}", l.name, l.version.as_deref().unwrap_or("?")),
                        (None, None) => "-".into(),
                    };
                    let line = format!("{}/{}  {}  {}  {}  {}", p.id, s.slug, s.name, s.state, s.game_version.as_deref().unwrap_or("?"), what);
                    println!("{}", show(&line));
                }
            }
        }
    }
    Ok(())
}

async fn sync_server(target: &str) -> Result<()> {
    let (client, paths) = (http_client(), Paths::new()?);
    let (panel, slug) = join::parse_target(target, &panel::list(&paths)?)?;
    let p = paths.clone();
    let (instance, _, manual) = with_progress(|tx| async move { join::sync(&client, &p, &panel, &slug, &tx).await }).await?;
    println!("instance {}", instance.id);
    print_manual(&paths, &instance, &manual);
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn wait_and_record(paths: &Paths, id: &str, child: &mut std::process::Child) -> Result<i32> {
    let started = unix_now();
    let code = child.wait()?.code().unwrap_or(1);
    let _ = instance::record_played(paths, id, started, unix_now().saturating_sub(started));
    Ok(code)
}

async fn join_server(target: &str) -> Result<()> {
    let (client, paths) = (http_client(), Paths::new()?);
    let profile = auth::current_profile(&paths).await?;
    let (panel, slug) = join::parse_target(target, &panel::list(&paths)?)?;
    let p = paths.clone();
    let (mut child, instance, manual, joined) = with_progress(|tx| async move { join::join(&client, &p, &panel, &slug, profile, &tx).await }).await?;
    print_manual(&paths, &instance, &manual);
    if !joined {
        println!("{}", join::NO_JOIN_NOTE);
    }
    println!("started, pid {}", child.id());
    let code = wait_and_record(&paths, &instance.id, &mut child)?;
    std::process::exit(code);
}

async fn pack_install(rest: &[String], pack: PackRef) -> Result<()> {
    let name = pack_name(rest, &pack)?;
    let (client, paths) = (http_client(), Paths::new()?);
    let p = paths.clone();
    let (instance, manual) = with_progress(|tx| async move { pack::install_new(&client, &p, &name, pack, None, &tx).await }).await?;
    println!("instance {}", instance.id);
    print_manual(&paths, &instance, &manual);
    Ok(())
}

async fn run(args: &[String]) -> Result<()> {
    match args {
        [cmd] if cmd == "versions" => {
            for v in portal_core::gamefiles::supported_versions(&manifest(&http_client()).await?) {
                if !v.snapshot {
                    println!("{}", v.id);
                }
            }
        }
        [cmd, id] if cmd == "files" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let version = version_json(&client, &paths, id).await?;
            with_progress(|tx| async move { install(&client, &paths, &version, None, &tx).await }).await?;
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
                println!("{}  {}  {}  {}  {}", i.id, show(&i.name), i.minecraft, loader, source);
            }
        }
        [cmd, sub, name, version, rest @ ..] if cmd == "instance" && sub == "create" => {
            let (client, paths) = (http_client(), Paths::new()?);
            let loader = match loader_option(rest)? {
                Some(l) => Some(loader::resolve_new(&client, version, &l).await?),
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
            let p = paths.clone();
            let (instance, manual) = with_progress(|tx| async move { pack::update(&client, &p, id, version, None, &tx).await }).await?;
            print_manual(&paths, &instance, &manual);
        }
        [cmd, sub, address, rest @ ..] if cmd == "panel" && sub == "add" => panel_add(address, allow_http(rest)?).await?,
        [cmd, sub] if cmd == "panel" && sub == "list" => {
            for p in panel::list(&Paths::new()?)? {
                println!("{}  {}  {}", p.id, show(&p.name), p.address);
            }
        }
        [cmd, sub, id] if cmd == "panel" && sub == "remove" => panel::remove(&Paths::new()?, id)?,
        [cmd] if cmd == "servers" => servers().await?,
        [cmd, target] if cmd == "sync" => sync_server(target).await?,
        [cmd, target] if cmd == "join" => join_server(target).await?,
        [cmd, id, rest @ ..] if cmd == "launch" => {
            let (server, memory) = launch_options(rest)?;
            let (client, paths) = (http_client(), Paths::new()?);
            let profile = auth::current_profile(&paths).await?;
            let instance = instance::load(&paths, id)?;
            let memory = memory.unwrap_or(instance.memory_mb());
            let p = paths.clone();
            let (mut child, _) = with_progress(|tx| async move {
                let tx = numbered(launch_stages(instance.loader.as_ref().map(|l| l.kind)), tx);
                start_instance(&client, &p, &instance, profile, server, memory, &tx).await
            }).await?;
            println!("started, pid {}", child.id());
            let code = wait_and_record(&paths, id, &mut child)?;
            std::process::exit(code);
        }
        [cmd] if cmd == "login" => login().await?,
        [cmd] if cmd == "logout" => auth::sign_out(&Paths::new()?)?,
        [cmd] if cmd == "whoami" => match settings::load(&Paths::new()?).account_name {
            Some(name) => println!("{}", show(&name)),
            None => println!("not signed in"),
        },
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
        let text = e.to_string();
        eprintln!("error: {}", text.lines().map(show).collect::<Vec<_>>().join("
"));
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
        let parsed = launch_options(&args(&["--server", "mc.test", "--memory", "2048"])).unwrap();
        assert_eq!(parsed, (Some("mc.test".into()), Some(2048)));
        assert_eq!(launch_options(&[]).unwrap(), (None, None));
        for bad in [&["--server"][..], &["--srever", "h"], &["--memory=8"], &["--name", "Bob"], &["--memory", "lots"], &["--dir", "x"], &["--loader", "fabric"]] {
            assert!(launch_options(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn panel_command_options() {
        assert!(!allow_http(&[]).unwrap());
        assert!(allow_http(&args(&["--allow-http"])).unwrap());
        for bad in [&["--allow-https"][..], &["--allow-http", "x"], &["x"]] {
            assert!(allow_http(&args(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(show("a\u{1b}[31mb\n"), "a [31mb ");
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
