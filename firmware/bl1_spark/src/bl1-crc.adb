with Interfaces;

package body BL1.CRC with SPARK_Mode is

   function CRC32 (Buf : Header_Buffer; Len : Natural) return U32 is
      Crc : U32 := 16#FFFF_FFFF#;
   begin
      for I in 0 .. Len - 1 loop
         Crc := Crc xor U32 (Buf (I));
         for Unused_Bit in 1 .. 8 loop
            if (Crc and 1) = 1 then
               Crc := Interfaces.Shift_Right (Crc, 1) xor 16#EDB8_8320#;
            else
               Crc := Interfaces.Shift_Right (Crc, 1);
            end if;
         end loop;
      end loop;
      return not Crc;
   end CRC32;

end BL1.CRC;