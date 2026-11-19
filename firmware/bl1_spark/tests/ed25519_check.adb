with Ada.Command_Line;
with Ada.Streams;
with Ada.Streams.Stream_IO;
with Ada.Text_IO;

with BL1.Ed25519;
with BL1.Types;

--  usage:
--    ed25519_check image <file> <public key: 64 hex digits>
--      Checks the signature in a CERBERUS image header (signed bytes 0 .. 71,
--      signature at offset 0x48).
--    ed25519_check raw <public key hex> <signature hex> <message hex>
--      Message up to 256 bytes, may be empty.
--  Prints Valid or Invalid.
procedure Ed25519_Check with SPARK_Mode => Off is
   use type Ada.Streams.Stream_Element_Offset;

   Signed_Length : constant := 16#48#;
   Sig_Offset    : constant := 16#48#;

   function Hex_Value (C : Character) return Natural is
   begin
      case C is
         when '0' .. '9' => return Character'Pos (C) - Character'Pos ('0');
         when 'a' .. 'f' => return Character'Pos (C) - Character'Pos ('a') + 10;
         when 'A' .. 'F' => return Character'Pos (C) - Character'Pos ('A') + 10;
         when others     => raise Constraint_Error with "bad hex digit";
      end case;
   end Hex_Value;

   function Hex_Byte (S : String; Index : Natural) return BL1.Types.Byte
   is (BL1.Types.Byte
         (Hex_Value (S (S'First + 2 * Index)) * 16
          + Hex_Value (S (S'First + 2 * Index + 1))));

   procedure Decode_Key (S : String; Key : out BL1.Ed25519.Public_Key) is
   begin
      if S'Length /= 2 * Key'Length then
         raise Constraint_Error with "public key must be 64 hex digits";
      end if;
      for I in Key'Range loop
         Key (I) := Hex_Byte (S, I);
      end loop;
   end Decode_Key;

   procedure Decode_Signature (S : String; Sig : out BL1.Ed25519.Signature) is
   begin
      if S'Length /= 2 * Sig'Length then
         raise Constraint_Error with "signature must be 128 hex digits";
      end if;
      for I in Sig'Range loop
         Sig (I) := Hex_Byte (S, I);
      end loop;
   end Decode_Signature;

   procedure Decode_Message
     (S      : String;
      Msg    : out BL1.Types.Header_Buffer;
      Length : out BL1.Types.U32)
   is
   begin
      Msg := (others => 0);
      Length := 0;
      if S'Length mod 2 /= 0 or else S'Length / 2 > BL1.Types.Header_Size then
         raise Constraint_Error with "bad message";
      end if;
      for I in 0 .. S'Length / 2 - 1 loop
         Msg (I) := Hex_Byte (S, I);
      end loop;
      Length := BL1.Types.U32 (S'Length / 2);
   end Decode_Message;

   procedure Read_Header (Path : String; Msg : out BL1.Types.Header_Buffer) is
      File : Ada.Streams.Stream_IO.File_Type;
      Raw  : Ada.Streams.Stream_Element_Array (1 .. BL1.Types.Header_Size);
      Last : Ada.Streams.Stream_Element_Offset;
   begin
      Msg := (others => 0);
      Ada.Streams.Stream_IO.Open (File, Ada.Streams.Stream_IO.In_File, Path);
      Ada.Streams.Stream_IO.Read (File, Raw, Last);
      Ada.Streams.Stream_IO.Close (File);
      if Last < Raw'Last then
         raise Constraint_Error with "image shorter than a header";
      end if;
      for I in Msg'Range loop
         Msg (I) := BL1.Types.Byte (Raw (Ada.Streams.Stream_Element_Offset (I) + 1));
      end loop;
   end Read_Header;

   procedure Report (Ok : Boolean) is
   begin
      if Ok then
         Ada.Text_IO.Put_Line ("Valid");
      else
         Ada.Text_IO.Put_Line ("Invalid");
      end if;
   end Report;

   Key : BL1.Ed25519.Public_Key := (others => 0);
   Sig : BL1.Ed25519.Signature  := (others => 0);
   Msg : BL1.Types.Header_Buffer := (others => 0);
   Len : BL1.Types.U32 := 0;
begin
   if Ada.Command_Line.Argument_Count = 4
     and then Ada.Command_Line.Argument (1) = "raw"
   then
      Decode_Key (Ada.Command_Line.Argument (2), Key);
      Decode_Signature (Ada.Command_Line.Argument (3), Sig);
      Decode_Message (Ada.Command_Line.Argument (4), Msg, Len);
      Report (BL1.Ed25519.Valid (Sig, Key, Msg, Len));

   elsif Ada.Command_Line.Argument_Count = 3
     and then Ada.Command_Line.Argument (1) = "image"
   then
      Decode_Key (Ada.Command_Line.Argument (3), Key);
      Read_Header (Ada.Command_Line.Argument (2), Msg);
      for I in Sig'Range loop
         Sig (I) := Msg (Sig_Offset + I);
      end loop;
      Report (BL1.Ed25519.Valid (Sig, Key, Msg, Signed_Length));

   else
      Ada.Text_IO.Put_Line ("usage: ed25519_check image <file> <key hex>");
      Ada.Text_IO.Put_Line ("       ed25519_check raw <key hex> <signature hex> <message hex>");
      Ada.Command_Line.Set_Exit_Status (Ada.Command_Line.Failure);
   end if;
exception
   when others =>
      Ada.Text_IO.Put_Line ("Error");
      Ada.Command_Line.Set_Exit_Status (Ada.Command_Line.Failure);
end Ed25519_Check;