with Ada.Command_Line;
with Ada.Streams;
with Ada.Streams.Stream_IO;
with Ada.Text_IO;

with BL1.SHA256;
with BL1.Types;

--  usage: sha256_check <file>
--  Prints the SHA-256 of the whole file as 64 lowercase hex digits.
procedure SHA256_Check with SPARK_Mode => Off is

   Hex_Digits : constant String := "0123456789abcdef";

   procedure Fail (Message : String) is
   begin
      Ada.Text_IO.Put_Line (Message);
      Ada.Command_Line.Set_Exit_Status (Ada.Command_Line.Failure);
   end Fail;

   procedure Run (Path : String) is
      File : Ada.Streams.Stream_IO.File_Type;
   begin
      Ada.Streams.Stream_IO.Open (File, Ada.Streams.Stream_IO.In_File, Path);
      declare
         Size : constant Natural := Natural (Ada.Streams.Stream_IO.Size (File));
      begin
         if Size > BL1.SHA256.Max_Data_Length then
            Ada.Streams.Stream_IO.Close (File);
            Fail ("TooLarge");
            return;
         end if;

         declare
            Raw    : Ada.Streams.Stream_Element_Array
                       (1 .. Ada.Streams.Stream_Element_Offset (Size));
            Last   : Ada.Streams.Stream_Element_Offset;
            Data   : BL1.Types.Byte_Array (0 .. Size - 1);
            Result : BL1.SHA256.Digest;
            Line   : String (1 .. 2 * BL1.SHA256.Digest_Size);
         begin
            Ada.Streams.Stream_IO.Read (File, Raw, Last);
            Ada.Streams.Stream_IO.Close (File);

            for I in Data'Range loop
               Data (I) :=
                 BL1.Types.Byte (Raw (Ada.Streams.Stream_Element_Offset (I) + 1));
            end loop;

            BL1.SHA256.Hash (Data, Result);

            for I in Result'Range loop
               Line (2 * I + 1) := Hex_Digits (Natural (Result (I)) / 16 + 1);
               Line (2 * I + 2) := Hex_Digits (Natural (Result (I)) mod 16 + 1);
            end loop;
            Ada.Text_IO.Put_Line (Line);
         end;
      end;
   end Run;

begin
   if Ada.Command_Line.Argument_Count /= 1 then
      Fail ("usage: sha256_check <file>");
   else
      Run (Ada.Command_Line.Argument (1));
   end if;
exception
   when others =>
      Fail ("IoError");
end SHA256_Check;