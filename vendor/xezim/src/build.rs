use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Capture the current git commit so the runtime banner can identify the
    // exact build (customer logs otherwise carry only the crate version, which
    // is not enough to pin a commit when triaging). Best-effort: an unknown or
    // missing git returns "unknown"; a dirty tree gets a "-dirty" suffix. Rerun
    // whenever HEAD moves so the baked-in hash stays current.
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|h| {
            // Only TRACKED modifications count as dirty — stray untracked
            // scratch files in the working tree say nothing about which source
            // the binary was built from.
            let dirty = Command::new("git")
                .args(["status", "--porcelain", "--untracked-files=no"])
                .output()
                .ok()
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);
            if dirty {
                format!("{}-dirty", h)
            } else {
                h
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=XEZIM_GIT_HASH={}", git_hash);
    // Commit timestamp (author/committer date, ISO-8601 strict) so a log can be
    // tied to a point in history, not just a hash. Best-effort; "unknown" if git
    // is absent or HEAD is unresolvable.
    let git_date = Command::new("git")
        .args(["show", "-s", "--format=%cI", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=XEZIM_GIT_DATE={}", git_date);
    // Nearest release tag reachable from HEAD, so `-V` can name the release a
    // build came from and not just its commit. `--abbrev=0` yields the bare
    // tag name (`0.10.3`) rather than the `tag-N-gHASH` long form — the hash
    // is printed alongside it already. Best-effort: an untagged history or a
    // missing git gives "unknown".
    let git_tag = Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=XEZIM_GIT_TAG={}", git_tag);
    // HEAD ref + index changes should retrigger the build script so the hash
    // does not go stale between commits.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=.git/refs/tags");

    // VPI symbols must be resolvable by dlopen'd DPI/VPI modules.
    //
    // The flag is spelled differently per linker: GNU/ELF ld takes
    // `-export-dynamic` (hyphen), Apple's ld takes `-export_dynamic`
    // (underscore) and — since the Xcode linker rewrite — rejects unknown
    // options as a hard error rather than warning, so the wrong spelling
    // breaks the build outright on macOS.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" | "ios" => {
            println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
        }
        // Windows has no equivalent (symbols are exported via a .def /
        // dllexport), and passing an unknown flag would fail the link.
        "windows" => {}
        _ => {
            println!("cargo:rustc-link-arg=-Wl,-export-dynamic");
        }
    }

    // vpi_printf and friends are C-variadic, which Rust cannot define on
    // stable (`c_variadic` is unstable). Compile a small C shim and link it
    // in. Invoked through `cc` directly rather than via the `cc` crate so
    // this adds no build dependency.
    let src = "src/vpi_printf_shim.c";
    println!("cargo:rerun-if-changed={}", src);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("vpi_printf_shim.o");
    let lib = out_dir.join("libvpishim.a");

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = Command::new(&cc)
        .args(["-c", "-fPIC", "-O2", src, "-o"])
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", cc, e));
    assert!(status.success(), "{} failed on {}", cc, src);

    let ar = std::env::var("AR").unwrap_or_else(|_| "ar".to_string());
    let _ = std::fs::remove_file(&lib);
    let status = Command::new(&ar)
        .arg("crs")
        .arg(&lib)
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", ar, e));
    assert!(status.success(), "{} failed", ar);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    // `+whole-archive` so vpi_printf is kept even though no Rust code
    // references it — a dlopen'd VPI module resolves it at load time.
    println!("cargo:rustc-link-lib=static:+whole-archive=vpishim");
}
