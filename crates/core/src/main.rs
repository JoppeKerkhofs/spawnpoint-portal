use portal_core::download::Progress;
use portal_core::gamefiles::{install, manifest, version_json};
use portal_core::{http_client, Paths, Result};
use std::io::Write;

const USAGE: &str = "usage:\n  portal versions         list the release versions, newest first\n  portal files <version>  download the game files of a version";

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
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
            let printer = tokio::spawn(async move {
                while let Some(p) = rx.recv().await {
                    print!("\r{} {}/{}", p.stage, p.done, p.total);
                    let _ = std::io::stdout().flush();
                }
                println!();
            });
            let result = install(&client, &paths, &version, &tx).await;
            drop(tx);
            let _ = printer.await;
            result?;
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
