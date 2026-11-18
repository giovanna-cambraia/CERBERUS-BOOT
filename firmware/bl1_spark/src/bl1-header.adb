with BL1.CRC;
with Interfaces;

package body BL1.Header with SPARK_Mode is

   function Get_U16 (B : Header_Buffer; Off : Natural) return U16
   with Pre => Off <= Header_Size - 2
   is
   begin
      return U16 (B (Off)) or Interfaces.Shift_Left (U16 (B (Off + 1)), 8);
   end Get_U16;

   function Get_U32 (B : Header_Buffer; Off : Natural) return U32
   with Pre => Off <= Header_Size - 4
   is
   begin
      return U32 (B (Off))
        or Interfaces.Shift_Left (U32 (B (Off + 1)), 8)
        or Interfaces.Shift_Left (U32 (B (Off + 2)), 16)
        or Interfaces.Shift_Left (U32 (B (Off + 3)), 24);
   end Get_U32;

   function Reserved_Is_Zero (B : Header_Buffer) return Boolean is
   begin
      for I in Off_Reserved .. Off_Crc - 1 loop
         if B (I) /= 0 then
            return False;
         end if;
      end loop;
      return True;
   end Reserved_Is_Zero;

   procedure Parse
     (Buf           : Header_Buffer;
      Expected_Type : U16;
      Status        : out Parse_Status;
      Fields        : out Header_Fields)
   is
      Image_Type   : U16;
      Payload_Size : U32;
   begin
      Fields :=
        (Image_Type       => 0,
         Security_Version => 0,
         Payload_Size     => 0,
         Load_Address     => 0,
         Entry_Point      => 0);

      if Get_U32 (Buf, Off_Magic) /= Magic then
         Status := Bad_Magic;
      elsif Get_U16 (Buf, Off_Version) /= Header_Version then
         Status := Unsupported_Version;
      elsif Get_U32 (Buf, Off_Crc) /= BL1.CRC.CRC32 (Buf, Off_Crc) then
         Status := Crc_Mismatch;
      elsif not Reserved_Is_Zero (Buf) then
         Status := Reserved_Not_Zero;
      else
         Image_Type := Get_U16 (Buf, Off_Type);
         if Image_Type /= Image_Type_BL2 and then Image_Type /= Image_Type_App then
            Status := Unknown_Image_Type;
         elsif Image_Type /= Expected_Type then
            Status := Wrong_Image_Type;
         else
            Payload_Size := Get_U32 (Buf, Off_Payload_Size);
            if Payload_Size = 0 or else Payload_Size > Max_Payload then
               Status := Bad_Payload_Size;
            else
               Fields :=
                 (Image_Type       => Image_Type,
                  Security_Version => Get_U32 (Buf, Off_Secver),
                  Payload_Size     => Payload_Size,
                  Load_Address     => Get_U32 (Buf, Off_Load),
                  Entry_Point      => Get_U32 (Buf, Off_Entry));
               Status := Ok;
            end if;
         end if;
      end if;
   end Parse;

   function Placement_Ok (Fields : Header_Fields) return Boolean is
      --  64-bit arithmetic: the sum of two 32-bit values cannot wrap.
      Load     : constant U64 := U64 (Fields.Load_Address);
      Size     : constant U64 := U64 (Fields.Payload_Size);
      Entry_Pt : constant U64 := U64 (Fields.Entry_Point);
   begin
      return Fields.Load_Address = BL2_Payload_Start
        and then Load + Size <= U64 (BL2_Region_End)
        and then Entry_Pt >= Load
        and then Entry_Pt < Load + Size;
   end Placement_Ok;

end BL1.Header;