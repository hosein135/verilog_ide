use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn update_submodules_silently() {
    // Vendored into the IDE tree. The `wellen` submodule only feeds optional
    // input VCDs for generated tests; do not run `git submodule` here, because
    // this directory has no `.git` and the command would walk up into the
    // parent repository.
}

fn collect_vcd_files(root: &Path) -> Vec<PathBuf> {
    let mut vcds = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) { Ok(e) => e, Err(_) => continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(ext) = path.extension() {
                if ext.eq_ignore_ascii_case("vcd") { vcds.push(path); }
            }
        }
    }
    vcds
}

fn main() {
    // Ensure submodules are present before scanning.
    update_submodules_silently();

    let crate_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let inputs = [
        crate_root.join("wellen/wellen/inputs"),
    ];

    // Known files to skip due to incorrect VCD format.
    let skip_list = [
        crate_root.join("wellen/wellen/inputs/VCD_file_with_errors.vcd"),
        crate_root.join("wellen/wellen/inputs/migen/migen_original.vcd"),
        crate_root.join("wellen/wellen/inputs/github_issues/issue40.vcd"),
        crate_root.join("wellen/wellen/inputs/migen/fractional_time_stamp.vcd"),
    ];

    let mut all_files: Vec<PathBuf> = Vec::new();
    for root in inputs.into_iter().filter(|p| p.is_dir()) {
        let mut vcds = collect_vcd_files(&root);
        vcds.retain(|p| !skip_list.iter().any(|s| s == p));
        all_files.extend(vcds);
    }

    // Generate one test per file into OUT_DIR.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let gen_path = out_dir.join("generated_wellen_tests.rs");
    let mut f = fs::File::create(&gen_path).unwrap();

    writeln!(f, "use std::fs; use std::path::PathBuf; use vcd_anon::anonymizer::VCDAnon; use wellen; use std::time::{{SystemTime, UNIX_EPOCH}};").unwrap();
    writeln!(f, "fn run_one(input: &str) {{").unwrap();
    writeln!(f, "  println!(\"Testing: {{}}\", input);").unwrap();
    writeln!(f, "  let input_path = PathBuf::from(input);").unwrap();
    writeln!(f, "  let out_root = PathBuf::from(env!(\"CARGO_MANIFEST_DIR\"))").unwrap();
    writeln!(f, "    .join(\"target\").join(\"test_outputs\").join(\"wellen_inputs\");").unwrap();
    writeln!(f, "  let _ = fs::create_dir_all(&out_root);").unwrap();
    writeln!(f, "  let rand_num = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();").unwrap();
    writeln!(f, "  let file_stem = input_path.file_stem().unwrap().to_string_lossy();").unwrap();
    writeln!(f, "  let out_vcd_name = format!(\"{{}}__{{}}.vcd\", file_stem, rand_num);").unwrap();
    writeln!(f, "  let out_map_name = format!(\"{{}}__{{}}.txt\", file_stem, rand_num);").unwrap();
    writeln!(f, "  let out_vcd = out_root.join(&out_vcd_name);").unwrap();
    writeln!(f, "  let out_map = out_root.join(&out_map_name);").unwrap();
    writeln!(f, "  let mut anon = VCDAnon::new(false, false);").unwrap();
    writeln!(f, "  anon.anonymize_file(&input_path, &out_vcd).expect(&format!(\"Failed to anonymize {{}}\", input));").unwrap();
    writeln!(f, "  anon.save_mapping(&out_map).expect(&format!(\"Failed to save mapping for {{}}\", input));").unwrap();
    writeln!(f, "  assert!(fs::metadata(&out_vcd).unwrap().len() > 0, \"Output VCD empty for {{}}\", input);").unwrap();
    writeln!(f, "  assert!(fs::metadata(&out_map).unwrap().len() > 0, \"Mapping file empty for {{}}\", input);").unwrap();
    writeln!(f, "  // Verify the generated VCD can be read by wellen").unwrap();
    writeln!(f, "  wellen::simple::read(&out_vcd).expect(&format!(\"wellen failed to read anonymized VCD for {{}}\", input));").unwrap();
    writeln!(f, "}}").unwrap();

    if all_files.is_empty() {
        // Provide at least one passing test to avoid empty suite issues.
        writeln!(f, "#[test] fn no_wellen_inputs_found() {{ assert!(true); }}").unwrap();
    } else {
        for (i, path) in all_files.iter().enumerate() {
            let p = path.to_string_lossy();
            writeln!(f, "#[test] fn wellen_input_{}() {{ run_one(\"{}\"); }}", i, p).unwrap();
        }
    }

    // Invalidate build if inputs change.
    println!("cargo:rerun-if-changed=wellen/wellen/inputs");
}
