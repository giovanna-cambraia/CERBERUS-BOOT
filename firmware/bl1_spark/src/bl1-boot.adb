with BL1.Ed25519;
with BL1.SHA256;

package body BL1.Boot with SPARK_Mode is
   use type BL1.Header.Parse_Status;
   use type BL1.Types.U32;

   --  Read the 32-byte vendor key from the start of the BL2 payload and check
   --  that its SHA-256 equals the hash in the OTP fuses.
   procedure Load_And_Check_Key
     (Otp     : BL1.SHA256.Digest;
      Key     : out BL1.Ed25519.Public_Key;
      Matches : out Boolean)
   with Global => (Input => BL1.Platform.Flash)
   is
      Blk : BL1.SHA256.Block;
      S   : BL1.SHA256.State;
      D   : BL1.SHA256.Digest;
   begin
      BL1.Platform.Read_Payload (0, BL1.Ed25519.Key_Size, Blk);
      Key := (others => 0);
      for I in Key'Range loop
         Key (I) := Blk (I);
      end loop;
      BL1.SHA256.Init (S);
      BL1.SHA256.Final (S, Blk, BL1.Ed25519.Key_Size, 32, D);
      Matches := BL1.SHA256.Equal (D, Otp);
   end Load_And_Check_Key;

   --  Stream the whole payload from flash through SHA-256 and compare with
   --  the digest in the header.
   procedure Payload_Matches
     (Size     : Natural;
      Expected : BL1.SHA256.Digest;
      Matches  : out Boolean)
   with
     Global => (Input => BL1.Platform.Flash),
     Pre    => Size <= BL1.Header.BL2_Payload_Capacity
   is
      S    : BL1.SHA256.State;
      Blk  : BL1.SHA256.Block;
      Tail : BL1.SHA256.Block;
      D    : BL1.SHA256.Digest;
      Full : constant Natural := Size / BL1.SHA256.Block_Size;
      Rest : constant Natural := Size mod BL1.SHA256.Block_Size;
   begin
      pragma Assert (Full * BL1.SHA256.Block_Size + Rest = Size);

      BL1.SHA256.Init (S);
      for I in 0 .. Full - 1 loop
         BL1.Platform.Read_Payload
           (I * BL1.SHA256.Block_Size, BL1.SHA256.Block_Size, Blk);
         BL1.SHA256.Update_Block (S, Blk);
      end loop;
      BL1.Platform.Read_Payload (Full * BL1.SHA256.Block_Size, Rest, Tail);
      BL1.SHA256.Final (S, Tail, Rest, BL1.Types.U64 (Size), D);
      Matches := BL1.SHA256.Equal (D, Expected);
   end Payload_Matches;

   procedure Check_BL2 (Result : out Report) is
      Buf           : BL1.Types.Header_Buffer;
      Fields        : BL1.Header.Header_Fields;
      Status        : BL1.Header.Parse_Status;
      Checks        : BL1.Policy.Check_Results := (others => False);
      Locked        : Boolean;
      Otp           : BL1.SHA256.Digest;
      Counter       : BL1.Types.U32 := 0;
      Version       : BL1.Types.U32 := 0;
      Key           : BL1.Ed25519.Public_Key := (others => 0);
      Key_Ok        : Boolean := False;
      Digest_Ok     : Boolean := False;
      Header_Digest : BL1.SHA256.Digest := (others => 0);
      Sig           : BL1.Ed25519.Signature := (others => 0);
      Entry_Point   : BL1.Types.U32 := 0;
      Failure       : Failure_Code;
      Decision      : BL1.Policy.Decision;
   begin
      BL1.Platform.Read_BL2_Header (Buf);
      BL1.Header.Parse (Buf, BL1.Header.Image_Type_BL2, Status, Fields);

      if Status = BL1.Header.Ok then
         Checks.Header_Ok := True;
         Version := Fields.Security_Version;

         if BL1.Header.Placement_Ok (Fields) then
            Checks.Placement_Ok := True;

            BL1.Platform.Read_RoT (Locked, Otp, Counter);
            if Locked then
               Checks.Provisioned := True;

               if Version >= Counter then
                  Checks.Rollback_Ok := True;

                  Load_And_Check_Key (Otp, Key, Key_Ok);
                  if Key_Ok then
                     Checks.Key_Hash_Ok := True;

                     for I in Header_Digest'Range loop
                        Header_Digest (I) := Buf (BL1.Header.Off_Digest + I);
                     end loop;
                     Payload_Matches
                       (Natural (Fields.Payload_Size), Header_Digest, Digest_Ok);
                     if Digest_Ok then
                        Checks.Digest_Ok := True;

                        for I in Sig'Range loop
                           Sig (I) := Buf (BL1.Header.Off_Signature + I);
                        end loop;
                        if BL1.Ed25519.Valid
                             (Sig, Key, Buf, BL1.Header.Signed_Length)
                        then
                           Checks.Signature_Ok := True;
                           Entry_Point := Fields.Entry_Point;
                        end if;
                     end if;
                  end if;
               end if;
            end if;
         end if;
      end if;

      Decision := BL1.Policy.Decide (Checks);

      Failure :=
        (if not Checks.Header_Ok then Header_Invalid
         elsif not Checks.Placement_Ok then Placement_Invalid
         elsif not Checks.Provisioned then Not_Provisioned
         elsif not Checks.Rollback_Ok then Rollback_Rejected
         elsif not Checks.Key_Hash_Ok then Key_Hash_Mismatch
         elsif not Checks.Digest_Ok then Digest_Mismatch
         elsif not Checks.Signature_Ok then Signature_Invalid
         else None);

      Result :=
        (Status      => Status,
         Failure     => Failure,
         Decision    => Decision,
         Entry_Point =>
           (if Decision = BL1.Policy.Jump_To_BL2 then Entry_Point else 0),
         Version     => Version,
         Counter     => Counter);
   end Check_BL2;

   procedure Finish_BL2 (R : in out Report) is
      Counter : BL1.Types.U32;
   begin
      if R.Decision = BL1.Policy.Jump_To_BL2 and then R.Version > R.Counter then
         BL1.Platform.Burn_BL2_Counter (R.Version);
         BL1.Platform.Read_BL2_Counter (Counter);
         R.Counter := Counter;
         if Counter /= R.Version then
            R.Decision := BL1.Policy.Safe_Halt;
            R.Failure := Counter_Burn_Failed;
            R.Entry_Point := 0;
         end if;
      end if;
   end Finish_BL2;

end BL1.Boot;