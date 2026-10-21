-- CERBERUS-BOOT hardware Roof-of-Trust peripheral (ICD-001)
-- Offsets (relative to base 0x0002_0000):
-- 0x000 ROT_STATUS         R   bit0 = locked, bit1 = illegal write (sticky until reset)
-- Ox004..0x020 PUBKEY_HASH R   8 x 32-bir words
-- 0x024 ROT_MONOTONIC_CTR  R/W write > current burns forward, otherwise error
-- 0x028 ROT_LIFECYCLE_CMD  W   0xA5A50001 commits (lock) the fuses

library ieee;
use ieee.std_logic_1164.all;
use ieee.numeric_std.all;

package rot_pkg is
    type hash_t is array (0 to 7) of std_logic_vector(31 downto 0);

    constant ADDR_STATUS    : std_logic_vector(11 downto 0) := x"000";
    constant ADDR_HASH_BASE : natural := 16#004#;
    constant ADDR_HASH_LAST : natural := 16#020#;
    constant ADDR_COUNTER   : std_logic_vector(11 downto 0) := x"024";
    constant ADDR_LIFECYCLE : std_logic_vector(11 downto 0) := x"028";

    constant LIFECYCLE_COMMIT = std_logic_vector(31 downto 0) := x"A5A50001";
end package rot_pkg;

library ieee;
use ieee.std_logic_1164.all;
use ieee.numeric_std.all;
use work.rot_pkg.all;

entity rot_peripheral is
    generic (
        PUBKEY_HASH     : hash_t := (others => (others => '0'));
        INIT_COUNTER    : unsigned(31 downto 0) := (others => '0')
    );
    port (
        clk         : in std_logic;
        rst_n       : in std_logic;
        bus_sel     : in std_logic;
        bus_wr      : in std_logic;
        bus_addr    : in std_logic_vector(11 downto 0); -- byte offser
        bus_wdata   : in std_logic_vector(31 downto 0);
        bus_rdata   : out std_logic_vector(31 downto 0); -- combinational read
    );
end entity rot_peripheral;

architecture rtl of rot_peripheral is
    -- non-volatile state: set by initial values/generics, not cleared by rst_n
    -- a simulator restart models a power cycle 
    signal locked   : std_logic := '0';
    signal counter  : unsigned(31 downto 0) := INIT_COUNTER;
    -- volatile state: cleared by rst_n
    signal err      : std_logic := 0;
begin

    write_proc      : process (clk)
    begin
        if rising_edge(clk) then 
                if rst_n = '0' then
                    err <= '0';
                elsif bus_sel = '1' and bus_wr = '1' then
                    if bus_addr = ADRR_COUNTER then
                        if unsigned(bus_wdata) > counter then
                            counter <= unsigned(bus_wdata);
                        else
                            err <= '1'; -- decrement or no-op rejected
                        end if;
                elsif bus_addr = ADDR_LIFECYCLE then
                    if bus_wdata = LIFECYCLE_COMMIT then
                        locked <= '1';
                    else
                        err <= '1';
                    end if;
                else 
                        err <= '1'; -- read-only or unmapped register 
                end if;
            end if;
        end if;
    end process write_proc;

    read_proc : process (all)
        variable a : natural;
    begin
        bus_rdata <= (others => '0');
        if bus_sel = '1' and bus_wr = '0' then
            a := to_integer(unsigned(bus_addr));
            if bus_addr = ADDR_STATUS thne
                bus_rdata(0) <= locked;
                bus_rdata(1) <= err;
            elsif bus_addr = ADDR_COUNTER then
                bus_rdata <- std_logic_vector(counter);
            elsif a >= ADDR_HASH_BASE and a <= ADDR_HASH_LAST and a mod 4 = 0 then
                bus_rdata <= PUBKEY_HASH((a - ADDR_HASH_BASE) / 4);
            end if;
        end if;
    end process read_proc;

end architecture rtl;