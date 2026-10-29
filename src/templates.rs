//! Scaffold templates for new Verilog and VHDL files.

use crate::language::ProjectLanguage;

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

pub fn design_ident(name: &str) -> String {
    sanitize_ident(name)
}

/// Design file, its body, testbench file, and its body for a new project.
pub fn scaffold_pair(language: ProjectLanguage, project_name: &str) -> (String, String, String, String) {
    let ident = design_ident(project_name);
    match language {
        ProjectLanguage::Verilog => (
            format!("{ident}.v"),
            module_template(&ident),
            format!("{ident}_tb.v"),
            testbench_template(&format!("{ident}_tb")),
        ),
        ProjectLanguage::Vhdl => (
            format!("{ident}.vhd"),
            vhdl_entity_template(&ident),
            format!("{ident}_tb.vhd"),
            vhdl_testbench_template(&format!("{ident}_tb")),
        ),
    }
}

pub fn body_for(filename: &str, language: ProjectLanguage) -> String {
    let stem = file_stem(filename);
    match language {
        ProjectLanguage::Verilog => {
            if is_testbench_filename(filename) {
                testbench_template(&stem)
            } else if is_verilog_filename(filename) {
                module_template(&stem)
            } else {
                String::new()
            }
        }
        ProjectLanguage::Vhdl => {
            if is_testbench_filename(filename) {
                vhdl_testbench_template(&stem)
            } else if is_vhdl_filename(filename) {
                vhdl_entity_template(&stem)
            } else {
                String::new()
            }
        }
    }
}

pub fn vhdl_entity_template(name: &str) -> String {
    let safe = sanitize_ident(name);
    format!(
        r#"library ieee;
use ieee.std_logic_1164.all;

-- {safe}.vhd — RTL entity
entity {safe} is
  port (
    a : in  std_logic;
    b : in  std_logic;
    y : out std_logic
  );
end entity {safe};

architecture rtl of {safe} is
begin
  y <= a xor b;
end architecture rtl;
"#
    )
}

/// Testbench scaffold. yavs writes the FST from Run; the testbench does not call `$dumpfile`.
pub fn vhdl_testbench_template(name: &str) -> String {
    let safe = sanitize_ident(name);
    let dut = safe.trim_end_matches("_tb").to_string();
    format!(
        r#"library ieee;
use ieee.std_logic_1164.all;
use std.env.all;

-- {safe}.vhd — testbench (yavs writes {dut}.fst when you click Run)
entity {safe} is
end entity {safe};

architecture sim of {safe} is
  signal a : std_logic := '0';
  signal b : std_logic := '0';
  signal y : std_logic;
begin
  dut : entity work.{dut}
    port map (
      a => a,
      b => b,
      y => y
    );

  stimulus : process
  begin
    a <= '0'; b <= '0'; wait for 10 ns;
    a <= '1'; b <= '0'; wait for 10 ns;
    a <= '0'; b <= '1'; wait for 10 ns;
    a <= '1'; b <= '1'; wait for 10 ns;
    report "{dut}: PASS";
    finish(0);
    wait;
  end process;
end architecture sim;
"#
    )
}

fn is_verilog_filename(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".v") || n.ends_with(".sv")
}

fn is_vhdl_filename(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".vhd") || n.ends_with(".vhdl")
}

fn file_stem(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower
        .trim_end_matches(".vhdl")
        .trim_end_matches(".vhd")
        .trim_end_matches(".sv")
        .trim_end_matches(".vh")
        .trim_end_matches(".v")
        .to_string()
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
