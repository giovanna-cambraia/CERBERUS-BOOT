with BL1.Types; use BL1.Types;

package BL1.Ed25519 with SPARK_Mode is

   Key_Size       : constant := 32;
   Signature_Size : constant := 64;

   type Public_Key is array (0 .. Key_Size - 1)       of Byte with Convention => C;
   type Signature  is array (0 .. Signature_Size - 1) of Byte with Convention => C;

   --  Implemented by Monocypher (C), outside the proof (ADR-004). Returns 1 if
   --  Sig is a valid signature of the first Length bytes of Message under Key,
   --  and 0 otherwise. It reads nothing but its arguments.
   function Check_C
     (Sig     : Signature;
      Key     : Public_Key;
      Message : Header_Buffer;
      Length  : U32) return U32
   with
     Import,
     Convention    => C,
     External_Name => "cerberus_ed25519_check",
     Global        => null,
     Pre           => Length <= Header_Size;

   function Valid
     (Sig     : Signature;
      Key     : Public_Key;
      Message : Header_Buffer;
      Length  : U32) return Boolean
   with Pre => Length <= Header_Size;

end BL1.Ed25519;