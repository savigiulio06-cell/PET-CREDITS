use std::path::{Path, PathBuf};
use std::{env, fs};

/// Collect `pets/*/manifest.json` and, when present, the local-only
/// `pets-local/*/manifest.json` (id + name) into a Rust table, so the tray
/// menu always lists the skins that are bundled in the UI. A manifest with
/// `"default": true` sorts first (it is the default skin); the rest by id.
fn variants() -> Vec<(String, String)> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut out: Vec<(bool, String, String)> = Vec::new();
    for sub in ["../../pets", "../../pets-local"] {
        scan(&root.join(sub), &mut out);
    }
    out.sort();
    out.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// Add every valid skin folder under `pets`; a missing folder is fine.
fn scan(pets: &Path, out: &mut Vec<(bool, String, String)>) {
    let Ok(dirs) = fs::read_dir(pets) else {
        return;
    };
    println!("cargo:rerun-if-changed={}", pets.display());
    for dir in dirs.flatten() {
        let manifest = dir.path().join("manifest.json");
        println!("cargo:rerun-if-changed={}", manifest.display());
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        // Same rule as the UI: the skin id is its folder name, restricted to a safe alphabet.
        let folder = dir.file_name().to_string_lossy().into_owned();
        if folder.is_empty()
            || !folder
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            continue;
        }
        // The same id in both folders is listed once (the UI also keeps the first one).
        if out.iter().any(|o| o.1 == folder) {
            continue;
        }
        let name = json
            .get("name")
            .and_then(|n| n.as_str())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&folder);
        let is_default = json.get("default").and_then(|d| d.as_bool()) == Some(true);
        out.push((
            !is_default,
            folder.clone(),
            name.chars().take(40).collect::<String>(),
        ));
    }
}

fn main() {
    let mut code = String::from("pub static VARIANTS: &[(&str, &str)] = &[\n");
    for (id, name) in variants() {
        code.push_str(&format!("    ({id:?}, {name:?}),\n"));
    }
    code.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("variants.rs");
    fs::write(out, code).unwrap();
    tauri_build::build()
}
