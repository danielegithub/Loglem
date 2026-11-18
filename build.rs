//! Raccoglie le ricette (`src/recipes/*.json`) al momento della compilazione: per aggiungere
//! un programma basta un file nuovo in quella cartella, senza toccare il codice.
use std::{env, fs, path::Path};

fn main() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/recipes");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<_> = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut list = String::from("&[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_stem().unwrap().to_string_lossy();
        list.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    list.push(']');
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("recipes.rs");
    fs::write(out, list).unwrap();
}
