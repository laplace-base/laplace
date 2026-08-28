use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    bundle_lapps();
    tauri_build::build()
}

/// Generates the `LAPPS` table included by `laplace_app_lib::bundled_lapps` on mobile targets.
///
/// Mobile packages have no writable lapps directory next to the binary, so the backend components
/// found in `lapps/back/` are embedded into the binary the same way `frontend-dist` is embedded by
/// `tauri::generate_context!`. On desktop the table stays empty: lapps are read from the filesystem
/// path configured in `Laplace.toml`.
fn bundle_lapps() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is always set by cargo"));
    let destination = out_dir.join("bundled_lapps.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let entries = if matches!(target_os.as_str(), "android" | "ios") {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo"));
        let root = manifest_dir.join("../../lapps/back");
        println!("cargo:rerun-if-changed={}", root.display());
        collect_lapps(&root)
    } else {
        Vec::new()
    };

    let mut source = String::from("pub static LAPPS: &[(&str, &[u8])] = &[\n");
    for (name, path) in entries {
        println!("cargo:rerun-if-changed={}", path.display());
        source.push_str(&format!(
            "    ({name:?}, include_bytes!({:?})),\n",
            path.display().to_string()
        ));
    }
    source.push_str("];\n");

    fs::write(&destination, source).unwrap_or_else(|err| {
        panic!("Failed to write '{}': {err}", destination.display());
    });
}

/// Collects `<root>/<name>/<name>-back.wasm` for every lapp directory under `root`.
fn collect_lapps(root: &Path) -> Vec<(String, PathBuf)> {
    let read_dir = match fs::read_dir(root) {
        Ok(read_dir) => read_dir,
        Err(err) => {
            println!(
                "cargo:warning=No lapps to bundle: cannot read '{}': {err}",
                root.display()
            );
            return Vec::new();
        },
    };

    let mut lapps = Vec::new();
    for entry in read_dir.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            println!(
                "cargo:warning=Skipping lapp with a non UTF-8 name in '{}'",
                root.display()
            );
            continue;
        };

        let component = entry.path().join(format!("{name}-back.wasm"));
        if component.is_file() {
            lapps.push((name, component));
        } else {
            println!(
                "cargo:warning=Skipping lapp '{name}': '{}' not found",
                component.display()
            );
        }
    }
    lapps.sort();
    lapps
}
