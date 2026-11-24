with Interfaces;
with System;

package body BL1.Platform with SPARK_Mode => Off is

   Report_Magic : constant U32 := 16#B101_0002#;

   type Report_Words  is array (0 .. 6) of U32;
   type RoT_Registers is array (0 .. 11) of U32;
   type Payload_Area  is array (0 .. BL1.Header.BL2_Payload_Capacity - 1) of Byte;

   --  ICD-001 word indexes: 0 status, 1 .. 8 key hash, 11 BL2 counter.
   BL2_Counter_Index : constant := 11;

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

   procedure Read_Payload
     (Offset : Natural;
      Count  : Natural;
      Buf    : out BL1.SHA256.Block)
   is
      Area : Payload_Area
        with Import, Volatile, Address => System'To_Address (16#4000_0100#);
   begin
      Buf := (others => 0);
      for I in 0 .. Count - 1 loop
         Buf (I) := Area (Offset + I);
      end loop;
   end Read_Payload;

   procedure Read_RoT
     (Locked      : out Boolean;
      Key_Hash    : out BL1.SHA256.Digest;
      BL2_Counter : out U32)
   is
      Regs : RoT_Registers
        with Import, Volatile, Address => System'To_Address (16#0002_0000#);
      Word : U32;
   begin
      Locked := (Regs (0) and 1) = 1;
      Key_Hash := (others => 0);
      for I in 0 .. 7 loop
         Word := Regs (1 + I);
         for K in 0 .. 3 loop
            Key_Hash (4 * I + K) :=
              Byte (Interfaces.Shift_Right (Word, 8 * K) and 16#FF#);
         end loop;
      end loop;
      BL2_Counter := Regs (BL2_Counter_Index);
   end Read_RoT;

   procedure Read_BL2_Counter (Value : out U32) is
      Regs : RoT_Registers
        with Import, Volatile, Address => System'To_Address (16#0002_0000#);
   begin
      Value := Regs (BL2_Counter_Index);
   end Read_BL2_Counter;

   procedure Burn_BL2_Counter (Value : U32) is
      Regs : RoT_Registers
        with Import, Volatile, Address => System'To_Address (16#0002_0000#);
   begin
      Regs (BL2_Counter_Index) := Value;
   end Burn_BL2_Counter;

   procedure Write_Report
     (Status      : U32;
      Failure     : U32;
      Decision    : U32;
      Entry_Point : U32;
      Version     : U32;
      Counter     : U32)
   is
      Report : Report_Words
        with Import, Volatile, Address => System'To_Address (16#2003_B000#);
   begin
      Report (1) := Status;
      Report (2) := Failure;
      Report (3) := Decision;
      Report (4) := Entry_Point;
      Report (5) := Version;
      Report (6) := Counter;
      Report (0) := Report_Magic;
   end Write_Report;

end BL1.Platform;