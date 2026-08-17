//! Generates the one list of files the installed binary carries and writes into every
//! generated tree's `vivarium/` subtree (ADR-0102): the product Nix tree, plus the crate
//! source the guest-agent derivation builds from. `nix/flake.nix` and `nix/flake.lock`
//! stay out — the flake file is the repository-side publication surface (and the sole
//! sanctioned edge to `tests/nix`), and the embedded tree is imported as a directory,
//! never entered as a flake.

use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const EXCLUDED: &[&str] = &["nix/flake.nix", "nix/flake.lock"];

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let mut relative_paths = vec![
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        "build.rs".to_owned(),
    ];
    for directory in ["nix", "src", "crates/vivarium-guest-agent"] {
        collect(&root, Path::new(directory), &mut relative_paths)?;
    }
    relative_paths.sort();
    relative_paths.dedup();

    let mut generated = String::from(concat!(
        "/// Everything the binary writes into a generated tree's `vivarium/` subtree, ",
        "sorted by path.\n",
        "pub const EMBEDDED_TREE: &[(&str, &[u8])] = &[\n",
    ));
    for relative in &relative_paths {
        let absolute = root.join(relative);
        println!("cargo:rerun-if-changed={}", absolute.display());
        writeln!(
            generated,
            "    ({relative:?}, include_bytes!({:?})),",
            absolute.display().to_string()
        )?;
    }
    generated.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("embedded_tree.rs"), generated)?;
    Ok(())
}

fn collect(root: &Path, directory: &Path, into: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    // Directories rerun the walk when an entry is added or removed; files alone would not.
    println!("cargo:rerun-if-changed={}", root.join(directory).display());
    for entry in fs::read_dir(root.join(directory))? {
        let entry = entry?;
        let path = directory.join(entry.file_name());
        let relative = path
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 path under `{}`", directory.display()))?
            .to_owned();
        if EXCLUDED.contains(&relative.as_str()) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect(root, &path, into)?;
        } else if kind.is_file() {
            into.push(relative);
        }
    }
    Ok(())
}
