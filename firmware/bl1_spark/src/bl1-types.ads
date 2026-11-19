with Interfaces;

package BL1.Types with SPARK_Mode is

   subtype Byte is Interfaces.Unsigned_8;
   subtype U16  is Interfaces.Unsigned_16;
   subtype U32  is Interfaces.Unsigned_32;
   subtype U64  is Interfaces.Unsigned_64;

   Header_Size : constant := 256;

   subtype Header_Index is Natural range 0 .. Header_Size - 1;

   --  The image header, copied out of flash into a local buffer. Parsing a
   --  fixed-size array keeps every index provable. The C convention lets the
   --  buffer be passed to C as a plain pointer.
   type Header_Buffer is array (Header_Index) of Byte with Convention => C;

   --  Index type is Integer (not Natural) so that empty arrays (0 .. -1) exist.
   type Byte_Array is array (Integer range <>) of Byte;

end BL1.Types;