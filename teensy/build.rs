use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    // cortex-m-rt の link.x が INCLUDE する memory.x を OUT_DIR にコピーし、
    // リンカ検索パスに追加する (MEMORY 領域はデバイス固有のため自前で用意)
    println!("cargo:rustc-link-search={}", out.display());
    fs::copy("memory.x", out.join("memory.x")).unwrap();
    println!("cargo:rerun-if-changed=memory.x");
}
