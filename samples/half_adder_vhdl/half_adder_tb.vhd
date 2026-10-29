library ieee;
use ieee.std_logic_1164.all;
use std.env.all;

-- Directed test for half_adder. yavs writes half_adder.fst when you press Run.
entity half_adder_tb is
end entity half_adder_tb;

architecture sim of half_adder_tb is
  signal a     : std_logic := '0';
  signal b     : std_logic := '0';
  signal sum   : std_logic;
  signal carry : std_logic;
begin
  dut : entity work.half_adder
    port map (
      a     => a,
      b     => b,
      sum   => sum,
      carry => carry
    );

  stimulus : process
    variable errors : integer := 0;
  begin
    a <= '0'; b <= '0'; wait for 10 ns;
    if sum /= '0' or carry /= '0' then
      errors := errors + 1;
    end if;
    report "a=" & std_logic'image(a) & " b=" & std_logic'image(b)
      & " sum=" & std_logic'image(sum) & " carry=" & std_logic'image(carry);

    a <= '0'; b <= '1'; wait for 10 ns;
    if sum /= '1' or carry /= '0' then
      errors := errors + 1;
    end if;
    report "a=" & std_logic'image(a) & " b=" & std_logic'image(b)
      & " sum=" & std_logic'image(sum) & " carry=" & std_logic'image(carry);

    a <= '1'; b <= '0'; wait for 10 ns;
    if sum /= '1' or carry /= '0' then
      errors := errors + 1;
    end if;
    report "a=" & std_logic'image(a) & " b=" & std_logic'image(b)
      & " sum=" & std_logic'image(sum) & " carry=" & std_logic'image(carry);

    a <= '1'; b <= '1'; wait for 10 ns;
    if sum /= '0' or carry /= '1' then
      errors := errors + 1;
    end if;
    report "a=" & std_logic'image(a) & " b=" & std_logic'image(b)
      & " sum=" & std_logic'image(sum) & " carry=" & std_logic'image(carry);

    if errors = 0 then
      report "half_adder: PASS";
    else
      report "half_adder: FAIL" severity error;
    end if;
    finish(0);
    wait;
  end process;
end architecture sim;
