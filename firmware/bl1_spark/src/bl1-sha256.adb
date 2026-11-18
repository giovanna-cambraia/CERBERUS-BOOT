with Interfaces;

package body BL1.SHA256 with SPARK_Mode is

   type Schedule is array (0 .. 63) of U32;
   type Constant_Table is array (0 .. 63) of U32;

   Round_Constants : constant Constant_Table :=
     (16#428A2F98#, 16#71374491#, 16#B5C0FBCF#, 16#E9B5DBA5#,
      16#3956C25B#, 16#59F111F1#, 16#923F82A4#, 16#AB1C5ED5#,
      16#D807AA98#, 16#12835B01#, 16#243185BE#, 16#550C7DC3#,
      16#72BE5D74#, 16#80DEB1FE#, 16#9BDC06A7#, 16#C19BF174#,
      16#E49B69C1#, 16#EFBE4786#, 16#0FC19DC6#, 16#240CA1CC#,
      16#2DE92C6F#, 16#4A7484AA#, 16#5CB0A9DC#, 16#76F988DA#,
      16#983E5152#, 16#A831C66D#, 16#B00327C8#, 16#BF597FC7#,
      16#C6E00BF3#, 16#D5A79147#, 16#06CA6351#, 16#14292967#,
      16#27B70A85#, 16#2E1B2138#, 16#4D2C6DFC#, 16#53380D13#,
      16#650A7354#, 16#766A0ABB#, 16#81C2C92E#, 16#92722C85#,
      16#A2BFE8A1#, 16#A81A664B#, 16#C24B8B70#, 16#C76C51A3#,
      16#D192E819#, 16#D6990624#, 16#F40E3585#, 16#106AA070#,
      16#19A4C116#, 16#1E376C08#, 16#2748774C#, 16#34B0BCB5#,
      16#391C0CB3#, 16#4ED8AA4A#, 16#5B9CCA4F#, 16#682E6FF3#,
      16#748F82EE#, 16#78A5636F#, 16#84C87814#, 16#8CC70208#,
      16#90BEFFFA#, 16#A4506CEB#, 16#BEF9A3F7#, 16#C67178F2#);

   function Ch (X, Y, Z : U32) return U32
   is ((X and Y) xor ((not X) and Z));

   function Maj (X, Y, Z : U32) return U32
   is ((X and Y) xor (X and Z) xor (Y and Z));

   function Big_Sigma0 (X : U32) return U32
   is (Interfaces.Rotate_Right (X, 2)
       xor Interfaces.Rotate_Right (X, 13)
       xor Interfaces.Rotate_Right (X, 22));

   function Big_Sigma1 (X : U32) return U32
   is (Interfaces.Rotate_Right (X, 6)
       xor Interfaces.Rotate_Right (X, 11)
       xor Interfaces.Rotate_Right (X, 25));

   function Small_Sigma0 (X : U32) return U32
   is (Interfaces.Rotate_Right (X, 7)
       xor Interfaces.Rotate_Right (X, 18)
       xor Interfaces.Shift_Right (X, 3));

   function Small_Sigma1 (X : U32) return U32
   is (Interfaces.Rotate_Right (X, 17)
       xor Interfaces.Rotate_Right (X, 19)
       xor Interfaces.Shift_Right (X, 10));

   procedure Init (S : out State) is
   begin
      S := (H => (16#6A09E667#, 16#BB67AE85#, 16#3C6EF372#, 16#A54FF53A#,
                  16#510E527F#, 16#9B05688C#, 16#1F83D9AB#, 16#5BE0CD19#));
   end Init;

   procedure Update_Block (S : in out State; B : Block) is
      W  : Schedule := (others => 0);
      Va, Vb, Vc, Vd, Ve, Vf, Vg, Vh : U32;
      T1, T2 : U32;
   begin
      --  Message schedule
      for T in 0 .. 15 loop
         W (T) :=
           Interfaces.Shift_Left (U32 (B (4 * T)), 24)
           or Interfaces.Shift_Left (U32 (B (4 * T + 1)), 16)
           or Interfaces.Shift_Left (U32 (B (4 * T + 2)), 8)
           or U32 (B (4 * T + 3));
      end loop;
      for T in 16 .. 63 loop
         W (T) := Small_Sigma1 (W (T - 2)) + W (T - 7)
                  + Small_Sigma0 (W (T - 15)) + W (T - 16);
      end loop;

      Va := S.H (0);
      Vb := S.H (1);
      Vc := S.H (2);
      Vd := S.H (3);
      Ve := S.H (4);
      Vf := S.H (5);
      Vg := S.H (6);
      Vh := S.H (7);

      --  Compression
      for T in 0 .. 63 loop
         T1 := Vh + Big_Sigma1 (Ve) + Ch (Ve, Vf, Vg) + Round_Constants (T) + W (T);
         T2 := Big_Sigma0 (Va) + Maj (Va, Vb, Vc);
         Vh := Vg;
         Vg := Vf;
         Vf := Ve;
         Ve := Vd + T1;
         Vd := Vc;
         Vc := Vb;
         Vb := Va;
         Va := T1 + T2;
      end loop;

      S.H (0) := S.H (0) + Va;
      S.H (1) := S.H (1) + Vb;
      S.H (2) := S.H (2) + Vc;
      S.H (3) := S.H (3) + Vd;
      S.H (4) := S.H (4) + Ve;
      S.H (5) := S.H (5) + Vf;
      S.H (6) := S.H (6) + Vg;
      S.H (7) := S.H (7) + Vh;
   end Update_Block;

   procedure Final
     (S            : in out State;
      Tail         : Block;
      Tail_Length  : Natural;
      Total_Length : U64;
      Result       : out Digest)
   is
      Blk  : Block := (others => 0);
      Bits : constant U64 := Total_Length * 8;
   begin
      for K in 0 .. Tail_Length - 1 loop
         Blk (K) := Tail (K);
      end loop;
      Blk (Tail_Length) := 16#80#;

      --  No room left for the 8-byte length: it goes in a second block.
      if Tail_Length >= 56 then
         Update_Block (S, Blk);
         Blk := (others => 0);
      end if;

      for K in 0 .. 7 loop
         Blk (56 + K) := Byte (Interfaces.Shift_Right (Bits, 8 * (7 - K)) and 16#FF#);
      end loop;
      Update_Block (S, Blk);

      Result := (others => 0);
      for I in 0 .. 7 loop
         for K in 0 .. 3 loop
            Result (4 * I + K) :=
              Byte (Interfaces.Shift_Right (S.H (I), 8 * (3 - K)) and 16#FF#);
         end loop;
      end loop;
   end Final;

   procedure Hash (Data : Byte_Array; Result : out Digest) is
      S    : State;
      Blk  : Block := (others => 0);
      Tail : Block := (others => 0);
      Full : constant Natural := Data'Length / Block_Size;
      Rest : constant Natural := Data'Length mod Block_Size;
   begin
      pragma Assert (Full * Block_Size + Rest = Data'Length);

      Init (S);
      for I in 0 .. Full - 1 loop
         for K in 0 .. Block_Size - 1 loop
            Blk (K) := Data (I * Block_Size + K);
         end loop;
         Update_Block (S, Blk);
      end loop;

      for K in 0 .. Rest - 1 loop
         Tail (K) := Data (Full * Block_Size + K);
      end loop;
      Final (S, Tail, Rest, U64 (Data'Length), Result);
   end Hash;

end BL1.SHA256;