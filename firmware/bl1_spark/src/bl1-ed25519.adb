package body BL1.Ed25519 with SPARK_Mode is

   function Valid
     (Sig     : Signature;
      Key     : Public_Key;
      Message : Header_Buffer;
      Length  : U32) return Boolean
   is (Check_C (Sig, Key, Message, Length) = 1);

end BL1.Ed25519;