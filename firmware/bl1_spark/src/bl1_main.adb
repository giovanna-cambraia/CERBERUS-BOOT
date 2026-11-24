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
   use type BL1.Policy.Decision;

   Result : BL1.Boot.Report;
begin
   BL1.Boot.Check_BL2 (Result);
   BL1.Boot.Finish_BL2 (Result);

   BL1.Platform.Write_Report
     (Status      => BL1.Types.U32 (BL1.Header.Parse_Status'Pos (Result.Status)),
      Failure     => BL1.Types.U32 (BL1.Boot.Failure_Code'Pos (Result.Failure)),
      Decision    => BL1.Types.U32 (BL1.Policy.Decision'Pos (Result.Decision)),
      Entry_Point => Result.Entry_Point,
      Version     => Result.Version,
      Counter     => Result.Counter);

   if Result.Decision = BL1.Policy.Jump_To_BL2 then
      BL1.Platform.Jump_To (Result.Entry_Point);
   end if;

   BL1.Platform.Halt;
end BL1_Main;