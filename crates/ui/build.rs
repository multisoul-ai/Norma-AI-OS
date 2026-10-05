use std::{collections::BTreeSet, env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=locales");
    let mut keys = BTreeSet::new();
    for entry in fs::read_dir("locales").expect("locale catalogs must exist") {
        let path = entry
            .expect("locale directory entry must be readable")
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let data: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&path).expect("locale catalog must be readable"),
        )
        .expect("locale catalog must contain valid JSON");
        for key in data
            .as_object()
            .expect("locale catalog must be a flat object")
            .keys()
        {
            keys.insert(key.clone());
        }
    }
    let source = format!(
        "pub static CATALOG_KEYS: &[&str] = &{:?};\n",
        keys.into_iter().collect::<Vec<_>>()
    );
    fs::write(
        Path::new(&env::var("OUT_DIR").expect("Cargo must set OUT_DIR")).join("i18n_keys.rs"),
        source,
    )
    .expect("locale key index must be writable");
}
