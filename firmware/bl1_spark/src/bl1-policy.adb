package body BL1.Policy with SPARK_Mode is

   function Decide (R : Check_Results) return Decision is
   begin
      if R.Header_Ok
        and then R.Placement_Ok
        and then R.Digest_Ok
        and then R.Key_Hash_Ok
        and then R.Signature_Ok
      then
         return Jump_To_BL2;
      else
         return Safe_Halt;
      end if;
   end Decide;

end BL1.Policy;