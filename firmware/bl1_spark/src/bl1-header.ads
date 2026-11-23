with BL1.Types; use BL1.Types;

package BL1.Header with SPARK_Mode is

   --  Image header, HDR_VERSION 1, 256 bytes, little-endian.
   --  Layout: firmware/common/spec/image_header.toml

   Magic          : constant U32 := 16#5345_4342#;
   Header_Version : constant U16 := 1;
   Image_Type_BL2 : constant U16 := 1;
   Image_Type_App : constant U16 := 2;
   Max_Payload    : constant U32 := 16#000D_FF00#;

   Off_Magic        : constant := 16#000#;
   Off_Version      : constant := 16#004#;
   Off_Type         : constant := 16#006#;
   Off_Secver       : constant := 16#008#;
   Off_Payload_Size : constant := 16#00C#;
   Off_Load         : constant := 16#010#;
   Off_Entry        : constant := 16#014#;
   Off_Salt         : constant := 16#018#;
   Off_Digest       : constant := 16#028#;
   Off_Signature    : constant := 16#048#;
   Off_Reserved     : constant := 16#088#;
   Off_Crc          : constant := 16#0FC#;

   --  The signature covers header bytes [0 .. Signed_Length - 1].
   Signed_Length : constant := Off_Signature;

   --  Where the BL2 image lives (ASM-003): a 128 KB partition whose first
   --  256 bytes are the header.
   BL2_Payload_Start : constant U32 := 16#4000_0100#;
   BL2_Region_End    : constant U32 := 16#4002_0000#;

   --  Size of the BL2 payload area: BL2_Region_End - BL2_Payload_Start.
   BL2_Payload_Capacity : constant := 16#1FF00#;

   type Header_Fields is record
      Image_Type       : U16;
      Security_Version : U32;
      Payload_Size     : U32;
      Load_Address     : U32;
      Entry_Point      : U32;
   end record;

   --  Verifier steps 1 to 6 of image_header.md, in this order.
   type Parse_Status is
     (Ok,
      Bad_Magic,
      Unsupported_Version,
      Crc_Mismatch,
      Reserved_Not_Zero,
      Unknown_Image_Type,
      Wrong_Image_Type,
      Bad_Payload_Size);

   procedure Parse
     (Buf           : Header_Buffer;
      Expected_Type : U16;
      Status        : out Parse_Status;
      Fields        : out Header_Fields)
   with
     Post =>
       (if Status = Ok then
          Fields.Image_Type = Expected_Type
          and then Fields.Payload_Size in 1 .. Max_Payload);

   --  The payload sits exactly at the start of the BL2 payload area, fits in
   --  the partition, and the entry point lies inside it (XIP only).
      function Placement_Ok (Fields : Header_Fields) return Boolean
   with
     Post =>
       (if Placement_Ok'Result then Fields.Payload_Size <= BL2_Payload_Capacity);

end BL1.Header;