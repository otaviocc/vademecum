//! Bakes the syntax set into a binary pack at build time.

use std::path::{Path, PathBuf};

use syntect::parsing::{SyntaxDefinition, SyntaxSet};

fn main() {
    let syntaxes = Path::new("syntaxes");
    println!("cargo::rerun-if-changed={}", syntaxes.display());
    println!("cargo::rerun-if-changed=build.rs");

    let mut builder = SyntaxSet::load_defaults_newlines().into_builder();

    let mut files: Vec<PathBuf> = std::fs::read_dir(syntaxes)
        .expect("the syntaxes directory is part of the repository")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sublime-syntax"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no syntaxes to bundle: syntaxes/ has no .sublime-syntax in it");

    for path in &files {
        let yaml = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let definition =
            SyntaxDefinition::load_from_str(&yaml, true, None).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        builder.add(definition);
    }

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("syntaxes.pack");
    syntect::dumps::dump_to_uncompressed_file(&builder.build(), &out)
        .unwrap_or_else(|error| panic!("{}: {error}", out.display()));
}
