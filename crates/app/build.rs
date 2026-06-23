use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let plist = manifest_dir.join("../../packaging/macos/DevInfo.plist");
    println!("cargo:rerun-if-changed={}", plist.display());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let plist = plist.canonicalize().unwrap_or(plist);
    println!(
        "cargo:rustc-link-arg-bin=multisoul=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
}
