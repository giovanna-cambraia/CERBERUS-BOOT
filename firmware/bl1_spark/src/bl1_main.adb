with BL1.Boot;
with BL1.Header;
with BL1.Platform;
with BL1.Policy;
with BL1.Types;

--  ROM entry, called by the start-up code (rt/start.S). Never returns.
procedure BL1_Main
with SPARK_Mode    => Off,
     Export,
     Convention    => C,
     External_Name => "bl1_main",
     No_Return
is
   Result : BL1.Boot.Report;
begin
   BL1.Boot.Check_BL2 (Result);

   BL1.Platform.Write_Report
     (Status    => BL1.Types.U32 (BL1.Header.Parse_Status'Pos (Result.Status)),
      Placement => (if Result.Placement then 1 else 0),
      Decision  => BL1.Types.U32 (BL1.Policy.Decision'Pos (Result.Decision)));

   loop
      null;
   end loop;
end BL1_Main;