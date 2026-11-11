//! Sends the game's log, its crash reports and this process's logcat to a computer on
//! the same network while the game runs, for headsets where USB debugging is not
//! available. Only in builds made with `HORIZON_LOG_URL` (a `POST` endpoint, such as
//! `tools/vr_dev_server.py`); others send nothing.
#![cfg(target_os = "android")]

use std::io::{BufRead, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const URL: Option<&str> = option_env!("HORIZON_LOG_URL");
const EVERY: Duration = Duration::from_secs(2);

/// Development builds (`HORIZON_LOG_URL`): `KEY=VALUE` lines from `vr-env.txt` beside the
/// log endpoint become environment variables, to try renderer switches on a headset
/// without a new build. Call before any other thread starts.
pub fn apply_remote_env() -> Vec<String> {
    let Some(url) = URL else {
        return Vec::new();
    };
    let base = url.trim_end_matches("/log");
    let Ok(text) = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(2))
        .build()
        .get(&format!("{base}/vr-env.txt"))
        .call()
        .and_then(|r| r.into_string().map_err(Into::into))
    else {
        return Vec::new();
    };
    let mut applied = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.starts_with("HORIZON_")
            && !key.contains(char::is_whitespace)
        {
            // SAFETY: called before the game starts any other thread.
            unsafe { std::env::set_var(key, value) };
            applied.push(format!("{key}={value}"));
        }
    }
    applied
}

pub fn start(home: &Path) {
    let Some(url) = URL else {
        return;
    };
    let home = home.to_path_buf();
    let (lines, logcat) = mpsc::channel::<String>();
    let _ = std::thread::Builder::new()
        .name("Logcat reader".into())
        .spawn(move || read_logcat(lines));
    let _ = std::thread::Builder::new()
        .name("Log uploader".into())
        .spawn(move || upload(url, &home, logcat));
}

fn read_logcat(lines: mpsc::Sender<String>) {
    let pid = std::process::id().to_string();
    let Ok(mut child) = std::process::Command::new("logcat")
        .args(["-v", "threadtime", "--pid", &pid])
        .stdout(std::process::Stdio::piped())
        .spawn()
    else {
        return;
    };
    let Some(out) = child.stdout.take() else {
        return;
    };
    for line in std::io::BufReader::new(out).lines() {
        let Ok(line) = line else { break };
        if lines.send(line).is_err() {
            break;
        }
    }
    let _ = child.kill();
}

fn upload(url: &str, home: &Path, logcat: mpsc::Receiver<String>) {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    let send = |name: &str, body: &[u8]| {
        if !body.is_empty() {
            let _ = agent
                .post(&format!("{url}?f={name}"))
                .send_bytes(body);
        }
    };
    send(
        "latest.log",
        format!(
            "\n===== Horizon launch, {} =====\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
        .as_bytes(),
    );
    let log = home.join("logs").join("latest.log");
    let crashes = home.join("crash-reports");
    let mut offset = 0u64;
    let mut seen: Vec<PathBuf> = list(&crashes);
    let eye = home.join("vr-eye.png");
    let mut eye_sent = None;
    loop {
        let modified = std::fs::metadata(&eye).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified != eye_sent {
            if let Ok(body) = std::fs::read(&eye) {
                send("eye.png", &body);
            }
            eye_sent = modified;
        }
        if let Ok(mut file) = std::fs::File::open(&log) {
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            if len < offset {
                offset = 0;
            }
            let mut chunk = Vec::new();
            if file.seek(SeekFrom::Start(offset)).is_ok() && file.read_to_end(&mut chunk).is_ok() {
                offset += chunk.len() as u64;
                send("latest.log", &chunk);
            }
        }
        let lines: Vec<String> = logcat.try_iter().collect();
        if !lines.is_empty() {
            send("logcat.txt", (lines.join("\n") + "\n").as_bytes());
        }
        for report in list(&crashes) {
            if !seen.contains(&report) {
                if let Ok(body) = std::fs::read(&report) {
                    send("crash.txt", &body);
                }
                seen.push(report);
            }
        }
        std::thread::sleep(EVERY);
    }
}

fn list(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| Some(e.ok()?.path())).collect())
        .unwrap_or_default()
}
