//! `--download`: the pinned Minecraft files, from Mojang, into the checkout's `.local/mojang`.
//!
//! Official URLs only (the version file named by its sha1 in `research-lock.json`, then the
//! client, server, libraries and assets that file lists). Each file is kept only when its
//! sha1 matches. Nothing is written outside `.local/`, and nothing of Mojang's is committed.
use horizon_prepare::pyjson::Value;
use sha1::{Digest, Sha1};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

const OBJECTS: &str = "https://resources.download.minecraft.net";

pub struct Fetched {
    pub minecraft_dir: PathBuf,
    pub server_jar: PathBuf,
}

struct FileGet {
    url: String,
    sha1: String,
    path: PathBuf,
}

/// Downloads the pinned version into `<root>/.local/mojang` (official launcher layout).
/// Files already there with the right sha1 are kept.
pub fn fetch(root: &Path, log: &(dyn Fn(&str) + Sync)) -> Result<Fetched, String> {
    let lock = horizon_prepare::artifacts::Lock::read(root)?;
    let minecraft = lock
        .json
        .get("minecraft")
        .ok_or("research-lock.json has no minecraft pin")?;
    let version = text(minecraft, "version")?;
    let manifest_sha1 = text(minecraft, "manifest_sha1")?;
    let client_sha1 = text(minecraft, "client_sha1")?;
    let server_sha1 = text(minecraft, "server_sha1")?;
    let dir = root.join(".local/mojang");
    let manifest_path = dir.join(format!("versions/{version}/{version}.json"));
    let manifest_url =
        format!("https://piston-meta.mojang.com/v1/packages/{manifest_sha1}/{version}.json");
    log(&format!(
        "Downloading Minecraft {version} from Mojang into {}",
        dir.display()
    ));
    fetch_one(
        &manifest_url,
        &manifest_sha1,
        &manifest_path,
        &ureq::Agent::new(),
    )?;
    let manifest = horizon_prepare::pyjson::loads(&fs::read_to_string(&manifest_path).map_err(|e| {
        format!("{}: {e}", manifest_path.display())
    })?)
    .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let mut files = game_files(
        &manifest,
        &dir,
        &version,
        os_name(),
        &client_sha1,
        &server_sha1,
    )?;
    let index = asset_index(&manifest, &dir)?;
    fetch_all(std::slice::from_ref(&index.file), 1, log)?;
    let index_json = horizon_prepare::pyjson::loads(&fs::read_to_string(&index.file.path).map_err(|e| {
        format!("{}: {e}", index.file.path.display())
    })?)
    .map_err(|e| format!("{}: {e}", index.file.path.display()))?;
    files.extend(asset_objects(&index_json, &dir)?);
    log(&format!(
        "Minecraft {version}: {} files (libraries and assets); ones already complete are skipped",
        files.len()
    ));
    fetch_all(&files, 8, log)?;
    let server = dir.join(format!("versions/{version}/server.jar"));
    Ok(Fetched {
        minecraft_dir: dir,
        server_jar: server,
    })
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn text(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing {key}"))
}

/// A path from a version file, refused when it could leave its folder.
fn relative(path: &str) -> Result<PathBuf, String> {
    let safe = !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    safe.then(|| PathBuf::from(path))
        .ok_or_else(|| format!("unsafe path {path:?} in the version file"))
}

fn official_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| format!("refusing a non-HTTPS URL ({url})"))?;
    let host = rest.split('/').next().unwrap_or("");
    let ok = host == "mojang.com"
        || host.ends_with(".mojang.com")
        || host == "minecraft.net"
        || host.ends_with(".minecraft.net");
    ok.then_some(())
        .ok_or_else(|| format!("refusing a URL that is not Mojang's ({url})"))
}

/// The last rule that names this OS (or no OS) decides. No rules: the library is for everyone.
fn allowed(library: &Value, os: &str) -> bool {
    let Some(rules) = library.get("rules").and_then(Value::as_array) else {
        return true;
    };
    rules
        .iter()
        .rfind(|rule| {
            rule.get("os")
                .and_then(|os| os.get("name"))
                .and_then(Value::as_str)
                .is_none_or(|name| name == os)
        })
        .is_some_and(|rule| rule.get("action").and_then(Value::as_str) == Some("allow"))
}

fn native_classifier(library: &Value, os: &str) -> Option<String> {
    let arch = if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    };
    library
        .get("natives")
        .and_then(|n| n.get(os))
        .and_then(Value::as_str)
        .map(|key| key.replace("${arch}", arch))
}

fn artifact(node: &Value, dir: &Path, under: &str) -> Result<Option<FileGet>, String> {
    if node.get("url").is_none() {
        return Ok(None);
    }
    let url = text(node, "url")?;
    official_url(&url)?;
    let sha1 = text(node, "sha1")?;
    let path = match node.get("path").and_then(Value::as_str) {
        Some(path) => dir.join(under).join(relative(path)?),
        None => return Ok(None),
    };
    Ok(Some(FileGet { url, sha1, path }))
}

fn game_files(
    manifest: &Value,
    dir: &Path,
    version: &str,
    os: &str,
    client_sha1: &str,
    server_sha1: &str,
) -> Result<Vec<FileGet>, String> {
    let downloads = manifest
        .get("downloads")
        .ok_or("the version file has no downloads")?;
    let mut files = Vec::new();
    for (name, pin) in [("client", client_sha1), ("server", server_sha1)] {
        let node = downloads
            .get(name)
            .ok_or_else(|| format!("the version file has no {name} download"))?;
        let url = text(node, "url")?;
        official_url(&url)?;
        let sha1 = text(node, "sha1")?;
        if sha1 != pin {
            return Err(format!(
                "the version file's {name} sha1 {sha1} is not the pinned {pin}"
            ));
        }
        files.push(FileGet {
            url,
            sha1,
            path: dir.join(match name {
                "client" => format!("versions/{version}/{version}.jar"),
                _ => format!("versions/{version}/server.jar"),
            }),
        });
    }
    let libraries = manifest
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or("the version file has no libraries")?;
    for library in libraries.iter().filter(|library| allowed(library, os)) {
        let downloads = library.get("downloads");
        if let Some(node) = downloads.and_then(|d| d.get("artifact")) {
            if let Some(file) = artifact(node, dir, "libraries")? {
                files.push(file);
            }
        }
        if let Some(key) = native_classifier(library, os) {
            if let Some(node) = downloads
                .and_then(|d| d.get("classifiers"))
                .and_then(|c| c.get(&key))
            {
                if let Some(file) = artifact(node, dir, "libraries")? {
                    files.push(file);
                }
            }
        }
    }
    Ok(files)
}

struct IndexFile {
    file: FileGet,
}

fn asset_index(manifest: &Value, dir: &Path) -> Result<IndexFile, String> {
    let index = manifest
        .get("assetIndex")
        .ok_or("the version file has no asset index")?;
    let url = text(index, "url")?;
    official_url(&url)?;
    let sha1 = text(index, "sha1")?;
    let id = text(index, "id")?;
    Ok(IndexFile {
        file: FileGet {
            url,
            sha1,
            path: dir.join(format!("assets/indexes/{id}.json")),
        },
    })
}

fn asset_objects(index: &Value, dir: &Path) -> Result<Vec<FileGet>, String> {
    let objects = index
        .get("objects")
        .and_then(Value::as_object)
        .ok_or("the asset index has no objects")?;
    let mut hashes: Vec<&str> = objects
        .iter()
        .filter_map(|(_, object)| object.get("hash").and_then(Value::as_str))
        .collect();
    hashes.sort_unstable();
    hashes.dedup();
    hashes
        .into_iter()
        .map(|hash| {
            if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("bad asset hash {hash:?}"));
            }
            let path = format!("{}/{hash}", &hash[..2]);
            Ok(FileGet {
                url: format!("{OBJECTS}/{path}"),
                sha1: hash.to_string(),
                path: dir.join("assets/objects").join(path),
            })
        })
        .collect()
}

fn fetch_all(files: &[FileGet], jobs: usize, log: &(dyn Fn(&str) + Sync)) -> Result<(), String> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let error: Mutex<Option<String>> = Mutex::new(None);
    let jobs = jobs.max(1).min(files.len().max(1));
    thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                let agent = ureq::AgentBuilder::new()
                    .timeout(std::time::Duration::from_secs(120))
                    .build();
                loop {
                    if error.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = files.get(i) else { return };
                    if let Err(e) = fetch_one(&file.url, &file.sha1, &file.path, &agent) {
                        *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                        return;
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n == files.len() || n % 250 == 0 {
                        log(&format!("Minecraft files: {n}/{}", files.len()));
                    }
                }
            });
        }
    });
    let failed = error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    match failed {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn fetch_one(url: &str, sha1: &str, path: &Path, agent: &ureq::Agent) -> Result<(), String> {
    official_url(url)?;
    if file_sha1(path).ok().as_deref() == Some(sha1) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let partial = path.with_extension(format!(
        "{}.partial",
        path.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let mut last = String::new();
    for attempt in 1..=3 {
        match download_to(url, &partial, agent) {
            Ok(()) => {
                let got = file_sha1(&partial)?;
                if got == sha1 {
                    fs::rename(&partial, path).map_err(|e| format!("{}: {e}", path.display()))?;
                    return Ok(());
                }
                let _ = fs::remove_file(&partial);
                last = format!("{url}: sha1 {got}, Mojang lists {sha1}");
            }
            Err(e) => last = e,
        }
        if attempt < 3 {
            thread::sleep(std::time::Duration::from_millis(400 * attempt));
        }
    }
    Err(last)
}

fn download_to(url: &str, path: &Path, agent: &ureq::Agent) -> Result<(), String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    if response.status() != 200 {
        return Err(format!("{url}: HTTP {}", response.status()));
    }
    let mut reader = response.into_reader();
    let mut file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    io::copy(&mut reader, &mut file).map_err(|e| format!("{}: {e}", path.display()))?;
    file.flush().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn file_sha1(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = Sha1::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            return Ok(hash
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect());
        }
        hash.update(&buffer[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_rule_keeps_linux_and_drops_windows() {
        let linux = horizon_prepare::pyjson::loads(
            r#"{"rules":[{"action":"allow"},{"action":"disallow","os":{"name":"windows"}}]}"#,
        )
        .unwrap();
        assert!(allowed(&linux, "linux"));
        let windows_only = horizon_prepare::pyjson::loads(
            r#"{"rules":[{"action":"allow","os":{"name":"windows"}}]}"#,
        )
        .unwrap();
        assert!(!allowed(&windows_only, "linux"));
        assert!(allowed(&windows_only, "windows"));
    }

    #[test]
    fn a_version_path_cannot_leave_the_folder() {
        assert!(relative("com/mojang/authlib/1.2/authlib-1.2.jar").is_ok());
        assert!(relative("../secret").is_err());
        assert!(relative("/etc/passwd").is_err());
    }

    #[test]
    fn only_mojang_https_urls_are_fetched() {
        assert!(official_url("https://piston-data.mojang.com/v1/objects/ab/server.jar").is_ok());
        assert!(official_url("https://resources.download.minecraft.net/ab/cd").is_ok());
        assert!(official_url("http://piston-data.mojang.com/x").is_err());
        assert!(official_url("https://example.com/client.jar").is_err());
    }
}
