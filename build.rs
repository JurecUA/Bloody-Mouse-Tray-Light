use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=resources/app.ico");
    println!("cargo:rerun-if-changed=resources/app.rc");
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let resource = out.join("app.res");
    let result = Command::new("rc.exe")
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg("resources/app.rc")
        .status()
        .expect("rc.exe is required to embed the application icon");
    assert!(result.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", resource.display());
}
