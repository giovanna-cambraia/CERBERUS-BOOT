library ieee;
use ieee.std_logic_1164.all;
use ieee.numeric_std.all;
use std.env.finish;
use work.rot_pkg.all;

entity rot_peripheral_tb is
end entity rot_peripheral_tb;

architecture sim of rot_peripheral_tb is
    constant HASH : hash_t := (
        x"00112233", x"44556677", x"8899AABB", x"CCDDEEFF",
        x"01234567", x"89ABCDEF", x"FEDCBA98", x"76543210");

    signal clk       : std_logic := '0';
    signal rst_n     : std_logic := '0';
    signal bus_sel   : std_logic := '0';
    signal bus_wr    : std_logic := '0';
    signal bus_addr  : std_logic_vector(11 downto 0) := (others => '0');
    signal bus_wdata : std_logic_vector(31 downto 0) := (others => '0');
    signal bus_rdata : std_logic_vector(31 downto 0);
begin
    clk <= not clk after 5 ns;

    dut : entity work.rot_peripheral
        generic map (PUBKEY_HASH => HASH)
        port map (
            clk => clk, rst_n => rst_n,
            bus_sel => bus_sel, bus_wr => bus_wr,
            bus_addr => bus_addr, bus_wdata => bus_wdata,
            bus_rdata => bus_rdata);

    stim : process
        variable rd : std_logic_vector(31 downto 0);

        procedure bus_write(addr : std_logic_vector(11 downto 0);
                            data : std_logic_vector(31 downto 0)) is
        begin
            bus_sel <= '1'; bus_wr <= '1'; bus_addr <= addr; bus_wdata <= data;
            wait until rising_edge(clk);
            wait for 1 ns;
            bus_sel <= '0'; bus_wr <= '0';
        end procedure;

        procedure bus_read(addr : std_logic_vector(11 downto 0);
                           data : out std_logic_vector(31 downto 0)) is
        begin
            bus_sel <= '1'; bus_wr <= '0'; bus_addr <= addr;
            wait for 1 ns;
            data := bus_rdata;
            bus_sel <= '0';
        end procedure;

        procedure expect(name : string; got, want : std_logic_vector(31 downto 0)) is
        begin
            assert got = want
                report "FAIL " & name & ": got " & to_hstring(got)
                       & ", want " & to_hstring(want)
                severity failure;
            report "ok   " & name;
        end procedure;

        procedure do_reset is
        begin
            rst_n <= '0';
            wait until rising_edge(clk);
            wait until rising_edge(clk);
            wait for 1 ns;
            rst_n <= '1';
        end procedure;
    begin
        do_reset;

        -- initial state
        bus_read(ADDR_STATUS, rd);  expect("T1 status after reset", rd, x"00000000");
        for i in 0 to 7 loop
            bus_read(std_logic_vector(to_unsigned(4 + 4 * i, 12)), rd);
            expect("T2 hash word " & integer'image(i), rd, HASH(i));
        end loop;
        bus_read(ADDR_COUNTER, rd); expect("T3 counter initial", rd, x"00000000");

        -- counter burns forward
        bus_write(ADDR_COUNTER, x"00000005");
        bus_read(ADDR_COUNTER, rd); expect("T4 counter = 5", rd, x"00000005");
        bus_read(ADDR_STATUS, rd);  expect("T4 no error", rd, x"00000000");

        -- decrement rejected, error flag set
        bus_write(ADDR_COUNTER, x"00000003");
        bus_read(ADDR_COUNTER, rd); expect("T5 counter still 5", rd, x"00000005");
        bus_read(ADDR_STATUS, rd);  expect("T5 error flag", rd, x"00000002");

        -- reset clears the error flag but not the counter
        do_reset;
        bus_read(ADDR_STATUS, rd);  expect("T6 error cleared", rd, x"00000000");
        bus_read(ADDR_COUNTER, rd); expect("T6 counter kept", rd, x"00000005");

        -- writing the same value is also rejected
        bus_write(ADDR_COUNTER, x"00000005");
        bus_read(ADDR_STATUS, rd);  expect("T7 equal write rejected", rd, x"00000002");
        do_reset;

        -- read-only hash register
        bus_write(x"004", x"DEADBEEF");
        bus_read(x"004", rd);       expect("T8 hash unchanged", rd, HASH(0));
        bus_read(ADDR_STATUS, rd);  expect("T8 error flag", rd, x"00000002");
        do_reset;

        -- bad lifecycle command does not lock
        bus_write(ADDR_LIFECYCLE, x"12345678");
        bus_read(ADDR_STATUS, rd);  expect("T9 bad cmd, not locked", rd, x"00000002");
        do_reset;

        -- correct commit locks, and the lock survives reset
        bus_write(ADDR_LIFECYCLE, LIFECYCLE_COMMIT);
        bus_read(ADDR_STATUS, rd);  expect("T10 locked", rd, x"00000001");
        do_reset;
        bus_read(ADDR_STATUS, rd);  expect("T11 lock kept after reset", rd, x"00000001");

        -- counter upper bound
        bus_write(ADDR_COUNTER, x"FFFFFFFF");
        bus_read(ADDR_COUNTER, rd); expect("T12 counter max", rd, x"FFFFFFFF");
        bus_write(ADDR_COUNTER, x"FFFFFFFF");
        bus_read(ADDR_STATUS, rd);  expect("T12 locked + error", rd, x"00000003");

        -- The BL2 counter is independent of the application counter
        bus_write(ADDR_BL2_COUNTER, x"00000003");
        bus_read(ADDR_BL2_COUNTER, rd); expect("T13 BL2 counter = 3", rd, x"00000003");
        bus_read(ADDR_COUNTER, rd);     expect("T13 app counter unchanged", rd, x"FFFFFFFF");
        bus_write(ADDR_BL2_COUNTER, x"00000002");
        bus_read(ADDR_BL2_COUNTER, rd); expect("T14 BL2 decrement rejected", rd, x"00000003");

        report "ALL TESTS PASSED";
        finish;
        wait;
    end process stim;
end architecture sim;
