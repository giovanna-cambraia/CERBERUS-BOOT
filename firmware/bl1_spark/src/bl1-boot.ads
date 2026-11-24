with BL1.Header;
with BL1.Platform;
with BL1.Policy;
with BL1.Types;

package BL1.Boot with SPARK_Mode is
   use type BL1.Policy.Decision;

   --  The first check that failed, in the order BL1 performs them.
   --  Counter_Burn_Failed is produced only by Finish_BL2.
   type Failure_Code is
     (None,
      Header_Invalid,
      Placement_Invalid,
      Not_Provisioned,
      Rollback_Rejected,
      Key_Hash_Mismatch,
      Digest_Mismatch,
      Signature_Invalid,
      Counter_Burn_Failed);

   type Report is record
      Status      : BL1.Header.Parse_Status;
      Failure     : Failure_Code;
      Decision    : BL1.Policy.Decision;
      Entry_Point : BL1.Types.U32;  --  where to jump; 0 unless Decision = Jump_To_BL2
      Version     : BL1.Types.U32;  --  BL2 security version (0 if the header was rejected)
      Counter     : BL1.Types.U32;  --  BL2 counter as last read (0 if never read)
   end record;

   --  Verify the BL2 image in flash. BL1 may jump only if no check failed.
   procedure Check_BL2 (Result : out Report)
   with
     Global => (Input => (BL1.Platform.Flash, BL1.Platform.RoT)),
     Post   => (Result.Decision = BL1.Policy.Jump_To_BL2) = (Result.Failure = None);

   --  Last step before the jump: raise the BL2 counter to the version just
   --  verified, and refuse to jump if the counter does not read back as raised.
   --  This can only make the outcome more restrictive.
   procedure Finish_BL2 (R : in out Report)
   with
     Global => (In_Out => BL1.Platform.RoT),
     Pre    => (R.Decision = BL1.Policy.Jump_To_BL2) = (R.Failure = None),
     Post   =>
       (R.Decision = BL1.Policy.Jump_To_BL2) = (R.Failure = None)
       and then (if R.Decision'Old = BL1.Policy.Safe_Halt
                 then R.Decision = BL1.Policy.Safe_Halt);

end BL1.Boot;