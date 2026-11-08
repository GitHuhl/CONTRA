//! Generated once from tools/ksp-conformance (cases.py, golden.json): KSP
//! init-time scripts and the integer results Kontakt 8 produced for them.
//! Only cases Kontakt compiled and ran to completion are kept.

pub const CASES: &[(&str, &str, &[&str], &[i32])] = &[
    (r#"arith_add_wrap"#, r#"declare $a := 2147483647"#, &[r#"$a + 1"#], &[-2147483648]),
    (r#"arith_sub_wrap"#, r#"declare $a := -2147483647"#, &[r#"$a - 2"#], &[2147483647]),
    (r#"arith_mul_wrap"#, r#"declare $a := 65536"#, &[r#"$a * 65536"#], &[0]),
    (r#"arith_mul_neg"#, r#"declare $a := -46341"#, &[r#"$a * $a"#], &[-2147479015]),
    (r#"arith_neg_min"#, r#"declare $a := -2147483647 - 1"#, &[r#"-$a"#, r#"$a - 1"#], &[-2147483648, 2147483647]),
    (r#"arith_div_trunc_pos"#, r#""#, &[r#"7 / 2"#, r#"-7 / 2"#], &[3, -3]),
    (r#"arith_mod_signs"#, r#""#, &[r#"-7 mod 2"#, r#"7 mod -2"#], &[-1, 1]),
    (r#"arith_mod_negneg"#, r#""#, &[r#"-7 mod -2"#], &[-1]),
    (r#"arith_abs"#, r#"declare $a := -2147483647"#, &[r#"abs($a)"#, r#"abs(-5)"#], &[2147483647, 5]),
    (r#"real_round_half"#, r#"declare ~h := 2.5
  declare ~n := -2.5"#, &[r#"real_to_int(round(~h))"#, r#"real_to_int(round(~n))"#], &[3, -3]),
    (r#"real_round_down"#, r#"declare ~h := 0.49999"#, &[r#"real_to_int(round(~h))"#], &[0]),
    (r#"real_to_int_trunc"#, r#""#, &[r#"real_to_int(3.9)"#, r#"real_to_int(-3.9)"#], &[3, -3]),
    (r#"real_to_int_big"#, r#"declare ~r := 2147483648.0"#, &[r#"real_to_int(~r)"#], &[2147483647]),
    (r#"real_int_real_exact"#, r#""#, &[r#"real_to_int(int_to_real(16777217))"#], &[16777217]),
    (r#"real_div_mixed"#, r#"declare ~x := 7.0"#, &[r#"real_to_int(~x / 2.0)"#, r#"real_to_int(~x / 2.0 * 2.0)"#], &[3, 7]),
    (r#"real_add_exact"#, r#""#, &[r#"real_to_int(2.5 + 2.5)"#], &[5]),
    (r#"shift_left_31"#, r#""#, &[r#"sh_left(1, 31)"#, r#"sh_left(3, 30)"#], &[-2147483648, -1073741824]),
    (r#"shift_right_negative"#, r#"declare $a := -8"#, &[r#"sh_right($a, 1)"#, r#"sh_right($a, 31)"#], &[-4, -1]),
    (r#"shift_right_positive"#, r#"declare $a := 1000"#, &[r#"sh_right($a, 3)"#, r#"sh_right($a, 0)"#], &[125, 1000]),
    (r#"logic_compare"#, r#"declare $a := 3
  declare $x := 0
  declare $y := 0
  if ($a > 2)
    $x := 1
  end if
  if ($a < 2)
    $y := 1
  end if"#, &[r#"$x"#, r#"$y"#], &[1, 0]),
    (r#"logic_in_range"#, r#"declare $a := 0
  declare $b := 0
  if (in_range(5, 1, 10))
    $a := 1
  end if
  if (in_range(11, 1, 10))
    $b := 1
  end if"#, &[r#"$a"#, r#"$b"#], &[1, 0]),
    (r#"incdec_sequence"#, r#"declare $a := 5
  inc($a)
  inc($a)
  dec($a)"#, &[r#"$a"#], &[6]),
    (r#"incdec_wrap"#, r#"declare $a := 2147483647
  inc($a)"#, &[r#"$a"#], &[-2147483648]),
    (r#"array_store_load"#, r#"declare %a[4]
  %a[2] := 7
  %a[3] := %a[2] * 3"#, &[r#"%a[3]"#, r#"%a[0]"#], &[21, 0]),
    (r#"array_num_elements"#, r#"declare %a[9]"#, &[r#"num_elements(%a)"#], &[9]),
    (r#"array_init_list"#, r#"declare %a[3] := (10, 20, 30)"#, &[r#"%a[1]"#, r#"%a[2] - %a[0]"#], &[20, 20]),
    (r#"array_out_of_bounds_read"#, r#"declare %a[3] := (1, 2, 3)"#, &[r#"%a[5]"#], &[0]),
    (r#"array_out_of_bounds_write"#, r#"declare %a[3] := (1, 2, 3)
  %a[7] := 9"#, &[r#"%a[2]"#], &[3]),
    (r#"loop_count"#, r#"declare $i := 0
  declare $s := 0
  while ($i < 10)
    $s := $s + $i
    inc($i)
  end while"#, &[r#"$s"#, r#"$i"#], &[45, 10]),
    (r#"loop_nested"#, r#"declare $i := 0
  declare $j := 0
  declare $s := 0
  while ($i < 4)
    $j := 0
    while ($j < 3)
      inc($s)
      inc($j)
    end while
    inc($i)
  end while"#, &[r#"$s"#], &[12]),
    (r#"cond_not_compare"#, r#"declare $t := 1
  declare $f := 0
  declare $a := 0
  declare $b := 0
  if (not ($f = 1))
    $a := 1
  end if
  if (not ($t = 1))
    $b := 1
  end if"#, &[r#"$a"#, r#"$b"#], &[1, 0]),
    (r#"cond_compare_ops"#, r#"declare $t := 1
  declare $a := 0
  declare $b := 0
  if ($t # 0)
    $a := 1
  end if
  if ($t > 0)
    $b := 1
  end if"#, &[r#"$a"#, r#"$b"#], &[1, 1]),
    (r#"builtin_msb_lsb"#, r#""#, &[r#"msb(1024)"#, r#"lsb(40)"#], &[8, 40]),
    (r#"builtin_msb_zero"#, r#""#, &[r#"msb(0)"#, r#"lsb(0)"#], &[0, 0]),
    (r#"builtin_sh_32"#, r#""#, &[r#"sh_left(1, 32)"#, r#"sh_right(-1, 40)"#], &[1, -1]),
    (r#"builtin_sh_negative"#, r#""#, &[r#"sh_left(1, -1)"#, r#"sh_right(8, -2)"#], &[-2147483648, 0]),
    (r#"builtin_pow"#, r#""#, &[r#"real_to_int(pow(2.0, 10.0))"#, r#"real_to_int(pow(2.0, -1.0) * 8.0)"#], &[1024, 4]),
    (r#"builtin_round_neg"#, r#""#, &[r#"real_to_int(round(-0.5))"#, r#"real_to_int(round(-1.5))"#], &[-1, -2]),
    (r#"builtin_int_conv"#, r#""#, &[r#"int(3.9)"#, r#"int(-3.9)"#], &[3, -3]),
    (r#"builtin_sqrt"#, r#""#, &[r#"real_to_int(sqrt(16.0))"#, r#"real_to_int(sqrt(2.0) * sqrt(2.0) * 1000.0)"#], &[4, 2000]),
    (r#"builtin_real_literal"#, r#""#, &[r#"real_to_int(1.5e3)"#, r#"real_to_int(0.1 * 10.0)"#], &[1500, 1]),
    (r#"arith_div_zero"#, r#"declare $z := 0"#, &[r#"7 / $z"#], &[0]),
    (r#"arith_mod_zero"#, r#"declare $z := 0"#, &[r#"7 mod $z"#], &[0]),
    (r#"arith_compound_neg"#, r#"declare $a := 5"#, &[r#"$a - (-$a)"#], &[10]),
    (r#"lit_int_max"#, r#"declare $a := 2147483647"#, &[r#"$a"#], &[2147483647]),
    (r#"lit_int_overflow"#, r#"declare $a := 2147483648"#, &[r#"$a"#], &[2147483647]),
    (r#"intval_bitand"#, r#""#, &[r#"6 .and. 3"#, r#"6 .or. 3"#], &[2, 7]),
    (r#"lit_u32_max"#, r#"declare $a := 4294967295"#, &[r#"$a"#], &[2147483647]),
    (r#"lit_u32_over"#, r#"declare $a := 4294967296"#, &[r#"$a"#], &[2147483647]),
    (r#"lit_3e9"#, r#"declare $a := 3000000000"#, &[r#"$a"#], &[2147483647]),
    (r#"lit_hex_top"#, r#"declare $a := 80000000h"#, &[r#"$a"#], &[-2147483648]),
    (r#"lit_neg_min"#, r#"declare $a := -2147483648"#, &[r#"$a"#], &[-2147483647]),
    (r#"lit_neg_over"#, r#"declare $a := -2147483649"#, &[r#"$a"#], &[-2147483647]),
    (r#"int_bitnot_dot"#, r#""#, &[r#".not. 5"#, r#".not. 0"#], &[-6, -1]),
    (r#"bool_xor_cond"#, r#"declare $a := 0
  declare $b := 0
  if ((1 = 1) xor (2 = 3))
    $a := 1
  end if
  if ((1 = 1) xor (2 = 2))
    $b := 1
  end if"#, &[r#"$a"#, r#"$b"#], &[1, 0]),
    (r#"array_search_short"#, r#"declare %a[5] := (4, 9, 2, 9, 1)"#, &[r#"search(%a, 9)"#], &[1]),
    (r#"sort_desc_two"#, r#"declare %a[4] := (3, 1, 4, 2)
  sort(%a, 1)"#, &[r#"%a[0]"#, r#"%a[3]"#], &[4, 1]),
    (r#"str_assign_decl"#, r#"declare @s
  @s := "abc""#, &[r#"0"#], &[0]),
    (r#"diag_decl_switch"#, r#"declare ui_switch $s"#, &[r#"0"#], &[0]),
    (r#"diag_set_switch"#, r#"declare ui_switch $s
  set_control_par($s, $CONTROL_PAR_VALUE, 1)"#, &[r#"0"#], &[0]),
    (r#"ui_var_knob_default"#, r#"declare ui_knob $k(0,127,1)"#, &[r#"$k"#], &[0]),
    (r#"ui_var_knob_set_high"#, r#"declare ui_knob $k(0,127,1)
  set_control_par($k, $CONTROL_PAR_VALUE, 150)"#, &[r#"$k"#], &[0]),
    (r#"ui_var_knob_set_neg"#, r#"declare ui_knob $k(0,127,1)
  set_control_par($k, $CONTROL_PAR_VALUE, -5)"#, &[r#"$k"#], &[0]),
    (r#"ui_var_switch_set"#, r#"declare ui_switch $s
  set_control_par($s, $CONTROL_PAR_VALUE, 1)"#, &[r#"$s"#], &[0]),
    (r#"diag_menu_add"#, r#"declare ui_menu $m
  add_menu_item($m, "a", 5)"#, &[r#"0"#], &[0]),
];
