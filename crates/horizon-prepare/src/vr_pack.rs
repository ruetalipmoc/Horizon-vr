//! `horizon-prepare vr-pack`: everything Horizon VR's first start asks for, in one step.
//! It fetches the pinned server jar from Mojang when none is at hand, prepares the
//! resources from the player's own Minecraft (the official launcher's copy, played once),
//! and zips what only the Java exporters make into `horizon-derived.zip` beside this
//! program, for the headset's setup screen. The zip is derived from Mojang's game: it is
//! for the player's own headset, never to be shared.
use std::io::Write;
use std::path::{Path, PathBuf};

/// What the headset takes straight from Mojang itself (`horizon_prepare::device`'s
/// outputs and the asset index), left out of the pack, as `make_derived_pack.sh` does.
fn skipped(path: &str) -> bool {
    const SKIP: &[&str] = &[
        "assets",
        "data",
        "pack.png",
        "asset-index.json",
        "block-tags.json",
        "damage-types.json",
        "damage-tags.json",
        "worldgen/structure",
        "classes",
        "client-classes",
    ];
    const WORLDGEN: &[&str] = &[
        "noises", "densities", "rules", "conditions", "settings", "carvers", "biomes",
        "world_presets", "features", "placed_features", "state_providers", "biome_tags",
        "feature_tags", "block_tags", "fluid_tags", "processor_lists", "template_pools",
        "structures", "structure_sets",
    ];
    SKIP.contains(&path)
        || WORLDGEN.iter().any(|n| path == format!("worldgen/{n}.json"))
        || path.ends_with(".log")
}

/// Zips `resources` into `out` without what `skipped` names. Returns the files written.
pub fn zip_derived(resources: &Path, out: &Path) -> Result<usize, String> {
    let partial = out.with_extension("zip.part");
    let file = std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0;
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = resources.join(&rel);
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries.into_iter().rev() {
            let child = rel.join(entry.file_name());
            let name = child.to_string_lossy().replace('\\', "/");
            if skipped(&name) {
                continue;
            }
            if entry.path().is_dir() {
                stack.push(child);
            } else {
                zip.start_file(name.as_str(), options).map_err(|e| e.to_string())?;
                let mut source =
                    std::fs::File::open(entry.path()).map_err(|e| format!("{name}: {e}"))?;
                std::io::copy(&mut source, &mut zip).map_err(|e| format!("{name}: {e}"))?;
                count += 1;
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    std::fs::rename(&partial, out).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(count)
}

/// The pinned server jar, fetched from Mojang into `root` when it is not there yet
/// (`curl` ships with Windows 10 and later, macOS and Linux). Its SHA-1 is checked by the
/// preparation itself.
pub fn server_jar(root: &Path, sha1: &str, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let jar = root.join("server.jar");
    if jar.is_file() {
        return Ok(jar);
    }
    let url = format!("https://piston-data.mojang.com/v1/objects/{sha1}/server.jar");
    log(&format!("Downloading the Minecraft server jar from Mojang ({url})"));
    let partial = root.join("server.jar.part");
    let status = std::process::Command::new("curl")
        .args(["-fL", "--retry", "3", "-o"])
        .arg(&partial)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl is needed to download the server jar: {e}"))?;
    if !status.success() {
        return Err(format!("could not download {url} ({status})"));
    }
    std::fs::rename(&partial, &jar).map_err(|e| e.to_string())?;
    Ok(jar)
}

/// Mojang's own version file for `version` (`sha1`, the pinned one), saved where the
/// preparation looks before the launcher's: some launchers rewrite theirs.
pub fn version_file(root: &Path, version: &str, sha1: &str, log: &dyn Fn(&str)) -> Result<(), String> {
    let dir = root.join(".local").join("minecraft").join(version);
    let file = dir.join("manifest.json");
    if file.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let url = format!("https://piston-meta.mojang.com/v1/packages/{sha1}/{version}.json");
    log(&format!("Downloading Mojang's version file ({url})"));
    let status = std::process::Command::new("curl")
        .args(["-fsL", "--retry", "3", "-o"])
        .arg(&file)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl is needed to download the version file: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&file);
        return Err(format!("could not download {url} ({status})"));
    }
    Ok(())
}

/// Waits for Enter, so a window opened by a double click stays to be read.
pub fn pause() {
    print!("\nPress Enter to close / Premi Invio per chiudere... ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
