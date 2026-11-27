use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::UNIX_EPOCH;

use serde::Serialize;

const OUTPUT: &str = "portal-output.log";
const TAIL: u64 = 256 * 1024;
// A file written this long before the recorded start still belongs to that run.
const SLACK_SECS: u64 = 5;

#[derive(Debug, Serialize, PartialEq)]
pub struct Section {
    pub name: String,
    pub text: String,
}

// Where the game's (stdout, stderr) go: a file in its logs folder, new at every start.
// Nothing is kept when that file cannot be made; the game starts either way.
pub fn capture(game_dir: &Path, stdout: bool) -> (Stdio, Stdio) {
    let logs = game_dir.join("logs");
    let Ok(file) = std::fs::create_dir_all(&logs).and_then(|_| File::create(logs.join(OUTPUT))) else {
        return (Stdio::null(), Stdio::null());
    };
    let out = if stdout { file.try_clone().map_or_else(|_| Stdio::null(), Stdio::from) } else { Stdio::null() };
    (out, Stdio::from(file))
}

fn modified(path: &Path) -> Option<u64> {
    Some(path.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

// Minecraft's access token is a JWT, and old versions print it ("Session ID is token:...").
fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("eyJ") {
        let word = &rest[at..];
        let len = word.find(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c))).unwrap_or(word.len());
        out.push_str(&rest[..at]);
        out.push_str(if len >= 40 { "<token>" } else { &word[..len] });
        rest = &word[len..];
    }
    out + rest
}

// The end of a file, from a whole line on; None for a file that is missing or empty.
fn tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let cut = len > TAIL;
    if cut {
        file.seek(SeekFrom::Start(len - TAIL)).ok()?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let text = if cut { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &text };
    (!text.trim().is_empty()).then(|| redact(text))
}

fn newest_crash_report(game_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .max_by_key(|p| modified(p))
}

// What the last run left behind, the most telling first. `since` is when that run started.
pub fn read(game_dir: &Path, since: u64) -> Vec<Section> {
    let fresh = |p: &Path| modified(p).is_some_and(|m| m + SLACK_SECS >= since);
    let logs = game_dir.join("logs");
    let files = [
        newest_crash_report(game_dir).filter(|p| fresh(p)),
        Some(logs.join(OUTPUT)),
        Some(logs.join("latest.log")).filter(|p| fresh(p)),
    ];
    files
        .into_iter()
        .flatten()
        .filter_map(|p| Some(Section { name: p.file_name()?.to_string_lossy().into_owned(), text: tail(&p)? }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_redacted_and_other_text_is_kept() {
        let token = format!("eyJ{}.eyJ{}.sig_-", "a".repeat(30), "b".repeat(30));
        assert_eq!(redact(&format!("(Session ID is token:{token}:uuid)\neyJ short")), "(Session ID is token:<token>:uuid)\neyJ short");
        assert_eq!(redact("plain"), "plain");
    }

    #[test]
    fn the_last_run_is_read_newest_crash_first_and_stale_files_are_left_out() {
        let dir = crate::download::testing::tmp_dir();
        assert!(read(&dir, 0).is_empty());

        drop(capture(&dir, true));
        std::fs::create_dir_all(dir.join("crash-reports")).unwrap();
        std::fs::write(dir.join("crash-reports/crash-1.txt"), "boom").unwrap();
        std::fs::write(dir.join("crash-reports/notes.md"), "no").unwrap();
        std::fs::write(dir.join("logs/latest.log"), "hello").unwrap();
        // The captured file is empty, so it has no section.
        let names = |since| read(&dir, since).into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names(0), ["crash-1.txt", "latest.log"]);
        std::fs::write(dir.join("logs").join(OUTPUT), "Error: could not create the Java Virtual Machine").unwrap();
        assert_eq!(names(0), ["crash-1.txt", OUTPUT, "latest.log"]);
        assert_eq!(names(u64::MAX), [OUTPUT]);
    }

    #[test]
    fn what_a_program_prints_is_kept() {
        let dir = crate::download::testing::tmp_dir();
        let (out, err) = capture(&dir, true);
        // The test program itself, asked for its help text.
        let status = std::process::Command::new(std::env::current_exe().unwrap()).arg("--help").stdout(out).stderr(err).status().unwrap();
        assert!(status.success());
        let sections = read(&dir, 0);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].name == OUTPUT && sections[0].text.contains("--help"), "{}", sections[0].text);
    }

    #[test]
    fn a_long_file_is_cut_to_whole_lines_at_its_end() {
        let dir = crate::download::testing::tmp_dir();
        let path = dir.join("big.log");
        std::fs::write(&path, "0123456789\n".repeat(TAIL as usize / 11 + 10) + "last").unwrap();
        let text = tail(&path).unwrap();
        assert!(text.len() as u64 <= TAIL && text.starts_with("0123456789\n") && text.ends_with("last"));
    }
}
