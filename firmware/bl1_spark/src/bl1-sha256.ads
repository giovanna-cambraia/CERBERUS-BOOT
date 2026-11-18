with BL1.Types; use BL1.Types;

package BL1.SHA256 with SPARK_Mode is

   --  SHA-256 (FIPS 180-4). Block-oriented, so BL1 can feed it straight from
   --  flash without a copy of the whole payload.

   Block_Size  : constant := 64;
   Digest_Size : constant := 32;

   --  Longest message Hash accepts (16 MB): keeps all index arithmetic far
   --  from overflow.
   Max_Data_Length : constant := 2 ** 24;

   type Block  is array (0 .. Block_Size - 1)  of Byte;
   type Digest is array (0 .. Digest_Size - 1) of Byte;

   type Word_Array is array (0 .. 7) of U32;

   type State is record
      H : Word_Array;
   end record;

   procedure Init (S : out State);

   --  Process one complete 64-byte block.
   procedure Update_Block (S : in out State; B : Block);

   --  Finish the hash. Tail holds the last Tail_Length (< 64) bytes of the
   --  message (the rest of Tail is ignored) and Total_Length is the length of
   --  the whole message in bytes.
   procedure Final
     (S            : in out State;
      Tail         : Block;
      Tail_Length  : Natural;
      Total_Length : U64;
      Result       : out Digest)
   with Pre => Tail_Length < Block_Size;

   --  Hash a whole buffer held in memory.
   procedure Hash (Data : Byte_Array; Result : out Digest)
   with Pre => Data'First = 0 and then Data'Length <= Max_Data_Length;

end BL1.SHA256;