with BL1.Header;
with BL1.Platform;
with BL1.Policy;

package BL1.Boot with SPARK_Mode is

   type Report is record
      Status    : BL1.Header.Parse_Status;
      Placement : Boolean;
      Decision  : BL1.Policy.Decision;
   end record;

   --  Check the BL2 image in flash. Part 1: header checks only. The digest,
   --  key hash and signature checks are placeholders until the crypto lands,
   --  so the decision is always Safe_Halt for now.
   procedure Check_BL2 (Result : out Report)
   with Global => (Input => BL1.Platform.Flash);

end BL1.Boot;