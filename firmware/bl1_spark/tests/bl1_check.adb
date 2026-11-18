with Ada.Command_Line;
with Ada.Streams;
with Ada.Streams.Stream_IO;
with Ada.Text_IO;

with BL1.Header;
with BL1.Types;

--  usage: bl1_check <image file> bl2|app
--  Prints the parse status (same names as test/vectors/manifest.toml) and,
--  when the header is accepted, the payload size.
procedure BL1_Check with SPARK_Mode => Off is
   use type Ada.Streams.Stream_Element_Offset;
   use type BL1.Header.Parse_Status;

   function Status_Name (S : BL1.Header.Parse_Status) return String is
   begin
      case S is
         when BL1.Header.Ok                  => return "Ok";
         when BL1.Header.Bad_Magic           => return "BadMagic";
         when BL1.Header.Unsupported_Version => return "UnsupportedVersion";
         when BL1.Header.Crc_Mismatch        => return "CrcMismatch";
         when BL1.Header.Reserved_Not_Zero   => return "ReservedNotZero";
         when BL1.Header.Unknown_Image_Type  => return "UnknownImageType";
         when BL1.Header.Wrong_Image_Type    => return "WrongImageType";
         when BL1.Header.Bad_Payload_Size    => return "BadPayloadSize";
      end case;
   end Status_Name;

   procedure Usage is
   begin
      Ada.Text_IO.Put_Line ("usage: bl1_check <image file> bl2|app");
      Ada.Command_Line.Set_Exit_Status (Ada.Command_Line.Failure);
   end Usage;

   Raw      : Ada.Streams.Stream_Element_Array (1 .. BL1.Types.Header_Size);
   Last     : Ada.Streams.Stream_Element_Offset;
   File     : Ada.Streams.Stream_IO.File_Type;
   Buf      : BL1.Types.Header_Buffer := (others => 0);
   Expected : BL1.Types.U16;
   Status   : BL1.Header.Parse_Status;
   Fields   : BL1.Header.Header_Fields;
begin
   if Ada.Command_Line.Argument_Count /= 2 then
      Usage;
      return;
   end if;

   if Ada.Command_Line.Argument (2) = "bl2" then
      Expected := BL1.Header.Image_Type_BL2;
   elsif Ada.Command_Line.Argument (2) = "app" then
      Expected := BL1.Header.Image_Type_App;
   else
      Usage;
      return;
   end if;

   Ada.Streams.Stream_IO.Open
     (File, Ada.Streams.Stream_IO.In_File, Ada.Command_Line.Argument (1));
   Ada.Streams.Stream_IO.Read (File, Raw, Last);
   Ada.Streams.Stream_IO.Close (File);

   if Last < Raw'Last then
      Ada.Text_IO.Put_Line ("TooShort");
      return;
   end if;

   for I in Buf'Range loop
      Buf (I) := BL1.Types.Byte (Raw (Ada.Streams.Stream_Element_Offset (I) + 1));
   end loop;

   BL1.Header.Parse (Buf, Expected, Status, Fields);
   Ada.Text_IO.Put_Line (Status_Name (Status));
   if Status = BL1.Header.Ok then
      Ada.Text_IO.Put_Line
        ("payload_size" & BL1.Types.U32'Image (Fields.Payload_Size));
   end if;
exception
   when others =>
      Ada.Text_IO.Put_Line ("IoError");
      Ada.Command_Line.Set_Exit_Status (Ada.Command_Line.Failure);
end BL1_Check;