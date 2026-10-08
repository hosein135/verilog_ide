`timescale 1ns / 1ps
// Directed test for half_adder. xezim writes half_adder.vcd when you press Run.
module half_adder_tb;
    reg        a;
    reg        b;
    wire       sum;
    wire       carry;
    reg [31:0] errors;

    half_adder dut (
        .a(a),
        .b(b),
        .sum(sum),
        .carry(carry)
    );

    initial begin
        $dumpfile("half_adder.vcd");
        $dumpvars(0, half_adder_tb);
        errors = 0;

        a = 1'b0; b = 1'b0; #10;
        if (sum !== 1'b0 || carry !== 1'b0) errors = errors + 1;
        $display("a=%b b=%b sum=%b carry=%b", a, b, sum, carry);

        a = 1'b0; b = 1'b1; #10;
        if (sum !== 1'b1 || carry !== 1'b0) errors = errors + 1;
        $display("a=%b b=%b sum=%b carry=%b", a, b, sum, carry);

        a = 1'b1; b = 1'b0; #10;
        if (sum !== 1'b1 || carry !== 1'b0) errors = errors + 1;
        $display("a=%b b=%b sum=%b carry=%b", a, b, sum, carry);

        a = 1'b1; b = 1'b1; #10;
        if (sum !== 1'b0 || carry !== 1'b1) errors = errors + 1;
        $display("a=%b b=%b sum=%b carry=%b", a, b, sum, carry);

        if (errors == 0)
            $display("half_adder: PASS");
        else
            $display("half_adder: FAIL (%0d)", errors);

        $dumpflush;
        $finish;
    end
endmodule
