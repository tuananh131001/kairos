use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let developer_dir = Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let candidates = [
        format!("{developer_dir}/usr/lib/swift/macosx"),
        format!("{developer_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx"),
        "/Library/Developer/CommandLineTools/usr/lib/swift/macosx".to_string(),
    ];
    for path in candidates {
        if Path::new(&path).join("libswiftCompatibility56.a").exists() {
            println!("cargo:rustc-link-search=native={path}");
            break;
        }
    }
}
