use portal_core::download::{Progress, ProgressTx};
use portal_core::gamefiles::{install, manifest, version_json};
use portal_core::java;
use portal_core::{http_client, Paths, Result};
use std::io::Write;

const USAGE: &str = "usage:\n  portal versions         list the release versions, newest first\n  portal files <version>  download the game files of a version
  portal java <component> download a Java runtime, e.g. java-runtime-delta";

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
        _ => println!("{USAGE}"),
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
