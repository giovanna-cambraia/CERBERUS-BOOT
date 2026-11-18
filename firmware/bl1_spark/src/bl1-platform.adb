with System;

package body BL1.Platform with SPARK_Mode => Off is

   Report_Magic : constant U32 := 16#B101_0001#;

   type Report_Words is array (0 .. 3) of U32;

   procedure Read_BL2_Header (Buf : out Header_Buffer) is
      Flash_Header : Header_Buffer
        with Import, Volatile, Address => System'To_Address (16#4000_0000#);
   begin
      --  Byte by byte, so the compiler cannot turn the copy into a call to
      --  a runtime function.
      for I in Buf'Range loop
         Buf (I) := Flash_Header (I);
      end loop;
   end Read_BL2_Header;

   procedure Write_Report (Status : U32; Placement : U32; Decision : U32) is
      Report : Report_Words
        with Import, Volatile, Address => System'To_Address (16#2003_B000#);
   begin
      Report (1) := Status;
      Report (2) := Placement;
      Report (3) := Decision;
      Report (0) := Report_Magic;
   end Write_Report;

end BL1.Platform;