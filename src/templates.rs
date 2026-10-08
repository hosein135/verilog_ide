//! Scaffold templates for new Verilog files.

pub fn module_template(name: &str) -> String {
    let safe = sanitize_ident(name);
    format!(
        r#"`timescale 1ns / 1ps
// {safe}.v — RTL module
module {safe} (
    input  wire a,
    input  wire b,
    output wire y
);

    assign y = a ^ b;

endmodule
"#
    )
}

/// Testbench scaffold with `$dumpfile` / `$dumpvars` so xezim writes a VCD.
pub fn testbench_template(name: &str) -> String {
    let safe = sanitize_ident(name);
    let dut = safe.trim_end_matches("_tb").to_string();
    let vcd = format!("{dut}.vcd");
    format!(
        r#"`timescale 1ns / 1ps
// {safe}.v — testbench (xezim writes {vcd} when you click Run)
module {safe};

    // Instantiate the DUT here, then drive stimulus below.

    initial begin
        $dumpfile("{vcd}");
        $dumpvars(0, {safe});
        #100;
        $dumpflush;
        $finish;
    end

endmodule
"#
    )
}

pub fn is_testbench_filename(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let stem = n.rsplit_once('.').map(|(s, _)| s).unwrap_or(&n);
    stem.ends_with("_tb")
        || stem.starts_with("tb_")
        || stem.ends_with("_testbench")
        || stem.ends_with("_test")
        || stem.contains("testbench")
}

fn sanitize_ident(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if i == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        }
    }
    if out.is_empty() {
        "design".into()
    } else {
        out
    }
}
