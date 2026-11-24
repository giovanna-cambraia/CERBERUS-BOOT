package BL1.Policy with SPARK_Mode is

   --  Outcome of every check BL1 performs on the BL2 image.
   type Check_Results is record
      Header_Ok    : Boolean;  --  steps 1-6 of the verifier order
      Placement_Ok : Boolean;  --  load address, size and entry point
      Provisioned  : Boolean;  --  the RoT fuses are committed
      Rollback_Ok  : Boolean;  --  BL2 version is not below the BL2 counter
      Key_Hash_Ok  : Boolean;  --  SHA-256 of BL2's key equals the OTP hash
      Digest_Ok    : Boolean;  --  SHA-256 of the payload equals the header digest
      Signature_Ok : Boolean;  --  Ed25519 over the signed header bytes
   end record;

   type Decision is (Jump_To_BL2, Safe_Halt);

   --  BL1 jumps to BL2 if and only if every check passed.
   function Decide (R : Check_Results) return Decision
   with
     Post =>
       (Decide'Result = Jump_To_BL2) =
         (R.Header_Ok
          and then R.Placement_Ok
          and then R.Provisioned
          and then R.Rollback_Ok
          and then R.Key_Hash_Ok
          and then R.Digest_Ok
          and then R.Signature_Ok);

end BL1.Policy;