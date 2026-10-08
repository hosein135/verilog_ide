use std::path::{Path, PathBuf};
use std::process::Command;

/// UVM 40phasing/06started_ended: the `main_phase` carries a
/// `static bit first=1` guard that, on the FIRST run, raises the objection,
/// `#10`, then sets `first=0` and `phase.jump(uvm_reset_phase::get())`. The
/// jump re-enters the reset -> ... -> main chain, so `main_phase` runs a
/// SECOND time — and it must observe `first==0` (the static latched through
/// the jump), so it does NOT jump again, the schedule completes, and the
/// phase-count counter reaches the golden 165.
///
/// Two xezim bugs previously livelocked this: (1) a `static` subroutine
/// local was kept as a per-recursion-frame copy (fixed in
/// `static_recursion_shared_cell`), and (2) even with live-cell routing the
/// persistent key was built from the *innermost* runtime sync frame, which a
/// UVM `wait_for`-style revival wrapper pollutes — so a resurrected
/// `main_phase` read/wrote `test::wait_for::first` instead of
/// `test::main_phase::first` and the latch never reached the store. The key
/// is now resolved from the routine that actually *declared* the static.
///
/// Discriminator: the test hangs/re-livspins (never prints PASSED) without
/// the fix; with it, `*** UVM TEST PASSED ***` (matches the reference simulator: 0 UVM_ERROR,
/// 0 UVM_FATAL).
#[test]
fn phase_jump_static_latch() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_dir = crate_dir.join("tests/scheduling");
    let test_file = test_dir.join("phase_jump_static_latch.sv");
    assert!(test_file.exists(), "Test file not found: {}", test_file.display());

    // Local UVM could be the git submodule at the repo root
    // (../1800.2-2020.3.1 relative to the crate). If absent, skip.
    let uvm_src = crate_dir.join("../1800.2-2020.3.1/src");
    if !uvm_src.join("uvm_pkg.sv").exists() {
        eprintln!("skipping: local UVM not present at {}", uvm_src.display());
        return;
    }

    let uvm_pkg = uvm_src.join("uvm_pkg.sv");
    let output = Command::new(env!("CARGO_BIN_EXE_xezim"))
        .arg("--simulate")
        .arg("-s")
        .arg("test")
        .arg("--max-time")
        .arg("2000ns")
        .arg("-DUVM_NO_DPI")
        .arg("-I")
        .arg(uvm_src.to_str().unwrap())
        .arg("-I")
        .arg(test_dir.to_str().unwrap())
        .arg("+UVM_TESTNAME=test")
        .arg(uvm_pkg.to_str().unwrap())
        .arg(test_file.to_str().unwrap())
        .output()
        .expect("Failed to execute xezim");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    assert!(
        combined.contains("*** UVM TEST PASSED ***"),
        "phase_jump_static_latch did not reach PASSED (static `first` did not latch \
         through the jump, or the scheduler did not complete).\nOutput:\n{combined}"
    );
}