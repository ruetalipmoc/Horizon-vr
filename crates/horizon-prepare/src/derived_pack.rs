//! The derived pack: what only the Java exporters make, out of a prepared resources folder,
//! for a device that cannot run them (Horizon VR's first start takes it as a zip, or makes it
//! itself where it ships a Java runtime). Kept in step with packaging/android/make_derived_pack.sh.
use std::path::{Path, PathBuf};

/// What the headset takes straight from Mojang itself (`horizon_prepare::device`'s
/// outputs and the asset index), left out of the pack, as `make_derived_pack.sh` does.
pub fn skipped(path: &str) -> bool {
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

