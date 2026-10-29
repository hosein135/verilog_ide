fn main() {
    println!("cargo:rerun-if-changed=assets/verilog-ide.ico");
    #[cfg(windows)]
    embed_icon();
    build_yavs();
}

/// Compile vendored yavs into its own target dir and expose the binary path.
/// A nested `cargo` invocation uses a different target directory so it does not
/// take the lock held by this build.
fn build_yavs() {
    println!("cargo:rerun-if-changed=vendor/yavs/Cargo.toml");
    println!("cargo:rerun-if-changed=vendor/yavs/Cargo.lock");
    println!("cargo:rerun-if-changed=vendor/yavs/crates");
    println!("cargo:rerun-if-changed=vendor/yavs/lib");

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let yavs_root = manifest_dir.join("vendor/yavs");
    let manifest = yavs_root.join("Cargo.toml");
    let target_dir = yavs_root.join("target");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());

    let mut cmd = std::process::Command::new(&cargo);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("-p")
        .arg("yavs")
        .arg("--locked")
        .current_dir(&yavs_root)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("CARGO_MAKEFLAGS")
        .env("CARGO_TARGET_DIR", &target_dir);
    if profile == "release" {
        cmd.arg("--release");
    }

    let output = cmd.output().unwrap_or_else(|err| {
        panic!("failed to start cargo for vendored yavs ({cargo}): {err}");
    });
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        panic!(
            "building vendored yavs failed\n{}\n{}",
            tail(&err),
            tail(&out)
        );
    }

    let exe = if cfg!(windows) { "yavs.exe" } else { "yavs" };
    let bin = target_dir.join(&profile).join(exe);
    if !bin.is_file() {
        panic!("yavs build succeeded but {} is missing", bin.display());
    }
    println!("cargo:rustc-env=YAVS_BIN={}", bin.display());
}

fn tail(text: &str) -> &str {
    const MAX: usize = 6000;
    if text.len() <= MAX {
        return text;
    }
    let mut start = text.len() - MAX;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(windows)]
fn embed_icon() {
    let icon = "assets/verilog-ide.ico";
    if !std::path::Path::new(icon).is_file() {
        println!("cargo:warning=missing {icon}; the executable will use the default icon");
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon);
    res.set("FileDescription", "Verilog IDE");
    res.set("ProductName", "Verilog IDE");
    res.set("OriginalFilename", "verilog-ide.exe");
    if let Err(err) = res.compile() {
        println!("cargo:warning=could not embed the application icon: {err}");
    }
}
