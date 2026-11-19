use portal_core::download::{Progress, ProgressTx};
use portal_core::gamefiles::{install, manifest, version_json};
use portal_core::java;
use portal_core::launch::{Profile, DEFAULT_MEMORY_MB};
use portal_core::play::{start, StartRequest};
use portal_core::{http_client, Paths, Result};
use std::io::Write;

const USAGE: &str = "usage:\n  portal versions         list the release versions, newest first\n  portal files <version>  download the game files of a version
  portal java <component> download a Java runtime, e.g. java-runtime-delta
  portal launch <version> --dir <folder> [--name <player>] [--server <host>] [--memory <MB>]
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

fn launch_request(version: &str, rest: &[String]) -> Result<StartRequest> {
    let (mut dir, mut name, mut server, mut memory) = (None, "Player".to_string(), None, DEFAULT_MEMORY_MB);
    let mut it = rest.iter();
    while let Some(f) = it.next() {
        let bad = || portal_core::Error::Other(format!("unknown or incomplete option: {f}"));
        let value = it.next().filter(|v| !v.starts_with("--")).ok_or_else(bad)?;
        match f.as_str() {
            "--dir" => dir = Some(value.clone()),
            "--name" => name = value.clone(),
            "--server" => server = Some(value.clone()),
            "--memory" => memory = value.parse().map_err(|_| portal_core::Error::Other(format!("invalid --memory: {value}")))?,
            _ => return Err(bad()),
        }
    }
    let dir = dir.ok_or_else(|| portal_core::Error::Other("--dir is required".into()))?;
    Ok(StartRequest { minecraft: version.to_string(), game_dir: dir.into(), profile: Profile::offline(&name), server, memory_mb: memory })
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
        [cmd, version, rest @ ..] if cmd == "launch" => {
            let request = launch_request(version, rest)?;
            let (client, paths) = (http_client(), Paths::new()?);
            let mut child = with_progress(|tx| async move { start(&client, &paths, &request, &tx).await }).await?;
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
    fn launch_options() {
        let r = launch_request("1.21.1", &args(&["--dir", "x y", "--name", "Bob", "--server", "mc.test", "--memory", "2048"])).unwrap();
        assert_eq!((r.game_dir, r.profile.name, r.server, r.memory_mb), ("x y".into(), "Bob".into(), Some("mc.test".into()), 2048));
        assert_eq!(launch_request("1.21.1", &args(&["--dir", "x"])).unwrap().memory_mb, DEFAULT_MEMORY_MB);
        for bad in [&["--dir", "x", "--server"][..], &["--dir", "x", "--srever", "h"], &["--dir", "x", "--memory=8"], &["--name", "--dir", "x"], &["--dir"], &[]] {
            assert!(launch_request("1.21.1", &args(bad)).is_err(), "{bad:?}");
        }
    }
}
