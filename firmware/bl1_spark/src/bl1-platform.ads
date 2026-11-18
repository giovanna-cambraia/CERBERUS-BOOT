with BL1.Types; use BL1.Types;

package BL1.Platform
  with SPARK_Mode,
       Abstract_State =>
         ((Flash       with External => Async_Writers),
          (Report_Area with External => Async_Readers))
is

   --  Hardware seen from the verified code. The body is not SPARK: it is the
   --  only place with memory-mapped access.

   --  Copy the 256-byte header at the start of the BL2 partition.
   procedure Read_BL2_Header (Buf : out Header_Buffer)
   with Global => (Input => Flash);

   --  Publish the boot report in SRAM for a debugger or the simulator: four
   --  words (magic, status, placement, decision). The magic word is written
   --  last, so a reader sees a complete report or none.
   procedure Write_Report (Status : U32; Placement : U32; Decision : U32)
   with Global => (Output => Report_Area);

end BL1.Platform;