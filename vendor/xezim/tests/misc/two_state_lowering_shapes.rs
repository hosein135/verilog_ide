//! Shapes the two-state lowering path (`lower_two_state`) admits, pinned by
//! VALUE so a miscompile is caught rather than just a coverage regression.
//! Each block below is X-free at its inputs, narrow and unsigned, so the
//! eval-site prefilter hands it to the two-state executor; the assertions
//! hold on either path, which is the point.
//!
//!  * §12.5 `case` compiled to a jump table (`CaseJump`) and to a masked
//!    bucket table (`CaseMaskJump`) — both dispatch on an X-free selector,
//!    so the 4-state "no pattern matches / wildcard chain" paths are dead.
//!  * §11.4.9 reduction `|` and `&`.
//!  * §11.5.1 dynamic bit-select TARGET, including an out-of-range index,
//!    which drops the write.
//!  * §5.7.1 `'x` in a `default:` arm: the constant cannot live in the
//!    two-state u64 register file, so it is folded at lowering time and the
//!    STORE writes the x plane.
//!  * A temp register redefined at DIFFERENT widths in different `case`
//!    arms — each use stays inside its own arm, so stream-order width
//!    tracking is exact and the block still lowers.

use xezim::simulate;

fn msgs(src: &str) -> Vec<String> {
    simulate(src, 1000)
        .expect("simulate failed")
        .output
        .iter()
        .map(|o| o.message.clone())
        .collect()
}

#[test]
fn case_jump_table_values() {
    let out = msgs(
        r#"
module top;
  logic [7:0] sel;
  logic [15:0] y;
  always_comb begin
    case (sel)
      8'd0:  y = 16'h1111;
      8'd7:  y = 16'h2222;
      8'd64: y = 16'h3333;
      8'd200:y = 16'h4444;
      8'd255:y = 16'h5555;
      default: y = 16'hDEAD;
    endcase
  end
  initial begin
    sel = 8'd0;   #1 $display("A_%04h", y);
    sel = 8'd7;   #1 $display("B_%04h", y);
    sel = 8'd64;  #1 $display("C_%04h", y);
    sel = 8'd200; #1 $display("D_%04h", y);
    sel = 8'd255; #1 $display("E_%04h", y);
    sel = 8'd123; #1 $display("F_%04h", y);
  end
endmodule
"#,
    );
    assert!(out.contains(&"A_1111".to_string()), "{out:?}");
    assert!(out.contains(&"B_2222".to_string()), "{out:?}");
    assert!(out.contains(&"C_3333".to_string()), "{out:?}");
    assert!(out.contains(&"D_4444".to_string()), "{out:?}");
    assert!(out.contains(&"E_5555".to_string()), "{out:?}");
    assert!(out.contains(&"F_dead".to_string()), "{out:?}");
}

#[test]
fn reduction_or_and() {
    let out = msgs(
        r#"
module top;
  logic [7:0] a;
  logic ro, ra;
  always_comb ro = |a;
  always_comb ra = &a;
  initial begin
    a = 8'h00; #1 $display("Z_%b%b", ro, ra);
    a = 8'h01; #1 $display("O_%b%b", ro, ra);
    a = 8'hFF; #1 $display("F_%b%b", ro, ra);
  end
endmodule
"#,
    );
    assert!(out.contains(&"Z_00".to_string()), "{out:?}");
    assert!(out.contains(&"O_10".to_string()), "{out:?}");
    assert!(out.contains(&"F_11".to_string()), "{out:?}");
}

#[test]
fn dynamic_bit_target_and_out_of_range() {
    // §11.5.1: an out-of-range index on an assignment TARGET drops the
    // write; the rest of the vector is untouched either way.
    let out = msgs(
        r#"
module top;
  logic [7:0] v;
  int idx;
  logic b;
  always_comb begin
    v = 8'h00;
    v[idx] = b;
  end
  initial begin
    idx = 0; b = 1'b1; #1 $display("A_%02h", v);
    idx = 3; b = 1'b1; #1 $display("B_%02h", v);
    idx = 7; b = 1'b1; #1 $display("C_%02h", v);
    idx = 9; b = 1'b1; #1 $display("D_%02h", v);
  end
endmodule
"#,
    );
    assert!(out.contains(&"A_01".to_string()), "{out:?}");
    assert!(out.contains(&"B_08".to_string()), "{out:?}");
    assert!(out.contains(&"C_80".to_string()), "{out:?}");
    assert!(out.contains(&"D_00".to_string()), "{out:?}");
}

#[test]
fn x_constant_default_arm_still_propagates_x() {
    // The folded constant must reach the signal as REAL x, not as 0.
    let out = msgs(
        r#"
module top;
  logic [3:0] s;
  logic [5:0] y;
  logic [7:0] w;
  always_comb begin
    case (s)
      4'd1: y = 6'h15;
      4'd2: y = 6'h2A;
      default: y = 6'bx;
    endcase
  end
  always_comb begin
    w = 8'h00;
    if (s == 4'd3) w[7:4] = 4'bx;
    else           w[7:4] = 4'hC;
  end
  initial begin
    s = 4'd1; #1 $display("A_%0h_%0h", y, w);
    s = 4'd2; #1 $display("B_%0h_%0h", y, w);
    s = 4'd9; #1 $display("C_%0h_%0h", y, w);
    s = 4'd3; #1 $display("D_%0h_%0h", y, w);
  end
endmodule
"#,
    );
    assert!(out.contains(&"A_15_c0".to_string()), "{out:?}");
    assert!(out.contains(&"B_2a_c0".to_string()), "{out:?}");
    // `%0h` of an all-x value is a single `x`; s=4'd3 matches no arm, so
    // `y` takes the default too — the x reaching BOTH a whole-signal store
    // and a [7:4] range store is exactly what the fold has to preserve.
    assert!(out.contains(&"C_x_c0".to_string()), "{out:?}");
    assert!(out.contains(&"D_x_x0".to_string()), "{out:?}");
}

#[test]
fn temp_reused_at_different_widths_per_arm() {
    // Each arm defines and consumes its own value; no use is reached by two
    // definitions, so the differing widths are safe to track in stream order.
    let out = msgs(
        r#"
module top;
  logic [2:0] s;
  logic [15:0] y;
  always_comb begin
    case (s)
      3'd0: y = {4{4'h3}};
      3'd1: y = {8{2'b10}};
      3'd2: y = {2{8'hA5}};
      3'd3: y = {16{1'b1}};
      default: y = 16'h0000;
    endcase
  end
  initial begin
    s = 3'd0; #1 $display("A_%04h", y);
    s = 3'd1; #1 $display("B_%04h", y);
    s = 3'd2; #1 $display("C_%04h", y);
    s = 3'd3; #1 $display("D_%04h", y);
    s = 3'd6; #1 $display("E_%04h", y);
  end
endmodule
"#,
    );
    assert!(out.contains(&"A_3333".to_string()), "{out:?}");
    assert!(out.contains(&"B_aaaa".to_string()), "{out:?}");
    assert!(out.contains(&"C_a5a5".to_string()), "{out:?}");
    assert!(out.contains(&"D_ffff".to_string()), "{out:?}");
    assert!(out.contains(&"E_0000".to_string()), "{out:?}");
}
