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
            let flag = |name: &str| rest.windows(2).find(|w| w[0] == name).map(|w| w[1].clone());
            let Some(dir) = flag("--dir") else {
                println!("{USAGE}");
                std::process::exit(1);
            };
            let memory_mb = match flag("--memory") {
                Some(m) => m.parse().map_err(|_| portal_core::Error::Other(format!("invalid --memory: {m}")))?,
                None => DEFAULT_MEMORY_MB,
            };
            let request = StartRequest {
                minecraft: version.clone(),
                game_dir: dir.into(),
                profile: Profile::offline(&flag("--name").unwrap_or_else(|| "Player".into())),
                server: flag("--server"),
                memory_mb,
            };
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
