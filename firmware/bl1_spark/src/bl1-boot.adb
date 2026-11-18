with BL1.Types;

package body BL1.Boot with SPARK_Mode is
   use type BL1.Header.Parse_Status;

   procedure Check_BL2 (Result : out Report) is
      Buf    : BL1.Types.Header_Buffer;
      Fields : BL1.Header.Header_Fields;
      Status : BL1.Header.Parse_Status;
      Placed : Boolean := False;
      Checks : BL1.Policy.Check_Results;
   begin
      BL1.Platform.Read_BL2_Header (Buf);
      BL1.Header.Parse (Buf, BL1.Header.Image_Type_BL2, Status, Fields);

      if Status = BL1.Header.Ok then
         Placed := BL1.Header.Placement_Ok (Fields);
      end if;

      Checks :=
        (Header_Ok    => Status = BL1.Header.Ok,
         Placement_Ok => Placed,
         Digest_Ok    => False,  --  not implemented yet
         Key_Hash_Ok  => False,  --  not implemented yet
         Signature_Ok => False); --  not implemented yet

      Result :=
        (Status    => Status,
         Placement => Placed,
         Decision  => BL1.Policy.Decide (Checks));
   end Check_BL2;

end BL1.Boot;