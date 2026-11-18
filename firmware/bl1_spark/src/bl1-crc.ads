with BL1.Types; use BL1.Types;

package BL1.CRC with SPARK_Mode is

   --  CRC-32/ISO-HDLC (the same as zlib's crc32) over the first Len bytes.
   function CRC32 (Buf : Header_Buffer; Len : Natural) return U32
   with Pre => Len <= Header_Size;

end BL1.CRC;