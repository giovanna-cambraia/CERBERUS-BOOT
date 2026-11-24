with BL1.Header;
with BL1.SHA256;
with BL1.Types; use BL1.Types;

package BL1.Platform
  with SPARK_Mode,
       Abstract_State =>
         ((Flash       with External => Async_Writers),
          (RoT         with External => (Async_Readers, Async_Writers)),
          (Report_Area with External => Async_Readers))
is

   --  Hardware seen from the verified code. The body is not SPARK: it is the
   --  only place with memory-mapped access.

   --  Copy the 256-byte header at the start of the BL2 partition.
   procedure Read_BL2_Header (Buf : out Header_Buffer)
   with Global => (Input => Flash);

   --  Copy Count bytes (at most one SHA-256 block) of the BL2 payload,
   --  starting Offset bytes after the payload start, into the start of Buf.
   --  The rest of Buf is zero.
   procedure Read_Payload
     (Offset : Natural;
      Count  : Natural;
      Buf    : out BL1.SHA256.Block)
   with
     Global => (Input => Flash),
     Pre    =>
       Count <= BL1.SHA256.Block_Size
       and then Offset <= BL1.Header.BL2_Payload_Capacity - Count;

   --  Lifecycle bit, the SHA-256 of the vendor key from the OTP fuses
   --  (ICD-001: word i holds digest bytes 4i .. 4i+3, little-endian), and the
   --  BL2 anti-rollback counter.
   procedure Read_RoT
     (Locked      : out Boolean;
      Key_Hash    : out BL1.SHA256.Digest;
      BL2_Counter : out U32)
   with Global => (Input => RoT);

   procedure Read_BL2_Counter (Value : out U32)
   with Global => (Input => RoT);

   --  Request a forward burn of the BL2 counter. The caller reads it back.
   procedure Burn_BL2_Counter (Value : U32)
   with Global => (In_Out => RoT);

   --  Publish the boot report in SRAM for a debugger or the simulator: seven
   --  words (magic, status, failure, decision, entry point, BL2 version,
   --  BL2 counter). The magic word is written last, so a reader sees a
   --  complete report or none.
   procedure Write_Report
     (Status      : U32;
      Failure     : U32;
      Decision    : U32;
      Entry_Point : U32;
      Version     : U32;
      Counter     : U32)
   with Global => (Output => Report_Area);

   --  Transfer control to the verified image. Never returns.
   --  Implemented in rt/jump.S.
   procedure Jump_To (Entry_Point : U32)
   with
     Import,
     Convention    => C,
     External_Name => "bl1_jump_to",
     No_Return,
     Global        => null;

   --  Fail stop. Never returns. Implemented in rt/jump.S.
   procedure Halt
   with
     Import,
     Convention    => C,
     External_Name => "bl1_halt",
     No_Return,
     Global        => null;

end BL1.Platform;