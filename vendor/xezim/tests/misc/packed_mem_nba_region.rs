//! Issue #147: under `XEZIM_PACKED_MEM=1`, NBAs into packed-arena cells
//! committed IMMEDIATELY (`packed.set_raw`) — blocking semantics — so a
//! same-timestep reader saw the new value a delta early. They now queue in
//! `packed_nba` and mature in the NBA region. Building the test also
//! exposed two latent panics on arena ids (the blocking fast-write path and
//! the AST-eval array-read fast path indexed per-signal tables), both fixed.
//!
//! Runs the binary in a subprocess: the packed flag and the name threshold
//! are read per-Simulator from the environment, and in-process env mutation
//! races parallel tests.
use std::process::Command;

fn xezim() -> String {
    let mut p = std::env::current_exe().expect("current_exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("xezim").to_string_lossy().into_owned()
}

const RACE: &str = r#"module top;
  logic clk = 0; always #5 clk = ~clk;
  logic [7:0] mem [0:63];
  logic [5:0] wa = 0, ra = 0; logic [7:0] wd = 8'hAA; logic we = 0;
  logic [7:0] q;
  always @(posedge clk) begin
    if (we) mem[wa] <= wd;
    q <= mem[ra];
  end
  initial begin
    mem[5] = 8'h11;                       // blocking fast-path write (panicked)
    wa = 5; ra = 5; we = 1;
    @(posedge clk); #1;
    $display("R1_%h_%h", q, mem[5]);      // AST-path read (panicked)
    we = 0; @(posedge clk); #1;
    $display("R2_%h", q);
    $finish;
  end
endmodule
"#;

#[test]
fn packed_nba_matures_in_nba_region() {
    let path = "/tmp/packed_nba_region_test.sv";
    std::fs::write(path, RACE).unwrap();
    let out = Command::new(xezim())
        .args(["--simulate", "-s", "top", path])
        .env("XEZIM_PACKED_MEM", "1")
        .env("XEZIM_LARGE_ARRAY_NAME_THRESHOLD", "16")
        .output()
        .expect("run xezim");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Same-cycle read must see the OLD value (reference-verified R1_11_aa);
    // the immediate-commit bug read back AA in the same edge.
    assert!(stdout.contains("R1_11_aa"), "packed NBA leaked early:\n{stdout}");
    assert!(stdout.contains("R2_aa"), "{stdout}");
    // And identical behavior with packing OFF (control).
    let out2 = Command::new(xezim())
        .args(["--simulate", "-s", "top", path])
        .output()
        .expect("run xezim");
    let s2 = String::from_utf8_lossy(&out2.stdout);
    assert!(s2.contains("R1_11_aa") && s2.contains("R2_aa"), "{s2}");
}
