use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // teensy4-bsp が提供する memory.x をリンカ検索パスに追加
    println!("cargo:rustc-link-search={}", out.display());
    fs::copy("memory.x", out.join("memory.x")).unwrap();
    println!("cargo:rerun-if-changed=memory.x");
}
