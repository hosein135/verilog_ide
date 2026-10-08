//! `--artifact-compression <none|1-22>`: the `-o` artifact can be written
//! raw (bincode, fastest load) or zstd-compressed at a chosen level; the
//! reader sniffs the zstd frame magic after the XEZIM header, so both kinds
//! load transparently.

use std::process::Command;

fn xezim_bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("xezim")
}

#[test]
fn artifact_modes_round_trip() {
    let bin = xezim_bin();
    if !bin.exists() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("xezim_artc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("t.sv"),
        "module tb;\n  logic [7:0] v = 8'hA7;\n  initial begin #1 $display(\"VAL %h\", v); $finish; end\nendmodule\n",
    )
    .expect("write");

    let compile = |out: &str, extra: &[&str]| {
        let mut c = Command::new(&bin);
        c.arg("--compile").arg(dir.join("t.sv")).args(["-s", "tb", "-o"]).arg(dir.join(out));
        c.args(extra);
        let o = c.output().expect("compile");
        assert!(
            String::from_utf8_lossy(&o.stdout).contains("Wrote compiled artifact"),
            "compile {out} failed:\n{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
    };
    compile("b_default", &[]);
    compile("b_none", &["--artifact-compression", "none"]);
    compile("b_19", &["--artifact-compression=19"]);

    let sz = |n: &str| std::fs::metadata(dir.join(n)).map(|m| m.len()).unwrap_or(0);
    assert!(sz("b_none") > sz("b_default"), "raw must be larger than zstd");

    for n in ["b_default", "b_none", "b_19"] {
        let o = Command::new(&bin)
            .arg("--simulate")
            .arg(dir.join(n))
            .output()
            .expect("simulate");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(text.contains("Loaded compiled:"), "{n}: fast path; got:\n{text}");
        assert!(text.contains("VAL a7"), "{n}: sim output; got:\n{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
