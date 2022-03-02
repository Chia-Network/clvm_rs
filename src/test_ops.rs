use crate::allocator::{Allocator, NodePtr, SExp};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_div_deprecated, op_divmod, op_gr,
    op_gr_bytes, op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not,
    op_point_add, op_pubkey_for_exp, op_sha256, op_softfork, op_strlen, op_substr, op_subtract,
};
use crate::number::{ptr_from_number, Number};
use crate::reduction::{Reduction, Response};
use hex::FromHex;
use num_traits::Num;
use std::collections::HashMap;

// the format of these test cases is the following. expected-cost is optional
// and is not relevant for FAIL cases

// expression => expected result | expected-cost
static TEST_CASES: &str = r#"
lognot ( 1 2 3 ) => FAIL
lognot 0xff => 0 | 334
lognot 0xffffff => 0 | 340
lognot 0x0000ff => 0xff00 | 360
lognot 0x0000000001 => 0xfe | 356
lognot 0xff00 => 0x00ff | 357
lognot 0x0c => 0xf3 | 344
lognot 0 => 0xff | 341
lognot 0xcccccc => 0x333333 | 370
lognot 0x333333 => 0xcccccc | 370
; requires exactly one argument
lognot 0x00 0x00 => FAIL
lognot => FAIL

logior ( 1 2 3 ) => FAIL
logior => 0 | 100
logior 0xbaadf00d => 0xbaadf00d | 416
logior 0xf0 0x0f => 0xff | 644
logior 0xcc 0x33 => 0xff | 644
logior 0x800000 0x01 => 0x800001 | 670
logior 0x400000 0x01 => 0x400001 | 670
logior 0x000040 0x01 => 0x41 | 650
logior 0x000080 0x01 => 0x0081 | 660
logior 0x000080 0xff => 0xff | 650
logior 0x000070 0xff => 0xff | 650
logior 0x000080 0x7f => 0x00ff | 660
logior 0xffff80 0x01 => 0x81 | 650
logior 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0xff | 2246
logior 0x01 0x01 => 0x01 | 644
logior 0x01 0x01 0x01 => 0x01 | 911

logxor ( 1 2 3 ) => FAIL
logxor => 0 | 100
logxor 0xbaadf00d => 0xbaadf00d | 416
logxor 0xf0 0x0f => 0xff | 644
logxor 0xcc 0x33 => 0xff | 644
logxor 0x800000 0x01 => 0x800001 | 670
logxor 0x400000 0x01 => 0x400001 | 670
logxor 0x000040 0x01 => 0x41 | 650
logxor 0x000080 0x01 => 0x0081 | 660
logxor 0x000080 0xff => 0xff7f | 660
logxor 0x000080 0x7f => 0x00ff | 660
logxor 0x000070 0xff => 0x8f | 650
logxor 0xffff80 0x01 => 0x81 | 650
logxor 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0xff | 2246
logxor 0x01 0x01 => 0 | 634
logxor 0x01 0x01 0x01 => 0x01 | 911

logand ( 1 2 3 ) => FAIL
logand => 0xff | 110
logand 0xbaadf00d => 0xbaadf00d | 416
logand 0xf0 0x0f => 0 | 634
logand 0xcc 0x33 => 0 | 634
logand 0x800000 0x01 => 0 | 640
logand 0x400000 0x01 => 0 | 640
logand 0x000040 0x01 => 0 | 640
logand 0x000080 0x01 => 0 | 640
; 0x000080 -> 0x0080, 0xff -> 0xffff
logand 0x000080 0xff => 0x0080 | 660
logand 0x000040 0xff => 0x40 | 650
logand 0x000080 0x7f => 0 | 640
logand 0x000070 0xff => 0x70 | 650
logand 0xffff80 0x01 => 0 | 640
logand 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0 | 2236
logand 0x01 0x01 => 0x01 | 644
logand 0x01 0x01 0x01 => 0x01 | 911

ash => FAIL
ash ( 1 2 3 ) 1 => FAIL
ash 0xffff ( 1 2 3 ) => FAIL
ash 0xff => FAIL
ash 0xff 1 => 0xfe | 612
ash 0xff 1 1 => FAIL

ash 0xff -1 => 0xff | 612
ash 0x80 -1 => 0xc0 | 612
ash 0x80 -2 => 0xe0 | 612
ash 0x80 -3 => 0xf0 | 612
ash 0x80 -4 => 0xf8 | 612
ash 0x80 -5 => 0xfc | 612
ash 0x80 -6 => 0xfe | 612
ash 0x80 -7 => 0xff | 612
ash 0x80 -8 => 0xff | 612

ash 0x7f -1 => 0x3f | 612
ash 0x7f -2 => 0x1f | 612
ash 0x7f -3 => 0x0f | 612
ash 0x7f -4 => 0x07 | 612
ash 0x7f -5 => 0x03 | 612
ash 0x7f -6 => 0x01 | 612
ash 0x7f -7 => 0 | 599
ash 0x7f -8 => 0 | 599

ash 0x80 1 => 0xff00 | 625
ash 0x80 2 => 0xfe00 | 625
ash 0x80 3 => 0xfc00 | 625
ash 0x80 4 => 0xf800 | 625
ash 0x80 5 => 0xf000 | 625
ash 0x80 6 => 0xe000 | 625
ash 0x80 7 => 0xc000 | 625
ash 0x80 8 => 0x8000 | 625

ash 0x7f 1 => 0x00fe | 622
ash 0x7f 2 => 0x01fc | 625
ash 0x7f 3 => 0x03f8 | 625
ash 0x7f 4 => 0x07f0 | 625
ash 0x7f 5 => 0x0fe0 | 625
ash 0x7f 6 => 0x1fc0 | 625
ash 0x7f 7 => 0x3f80 | 625
ash 0x7f 8 => 0x7f00 | 625

ash 0x90000000000000000000000000 -100 => -7 | 648
ash -7 100 => 0x90000000000000000000000000 | 768
ash 0xcc 1 => 0x98 | 612
ash 0xcc 2 => 0xff30 | 622
ash 0xcc 3 => 0xfe60 | 625
ash 0xcc 4 => 0xfcc0 | 625
ash 0xcc 5 => 0xf980 | 625
ash 0xcc 6 => 0xf300 | 625

ash 0xcc -1 => 0xe6 | 612
ash 0xcc -2 => 0xf3 | 612
ash 0xcc -3 => 0xf9 | 612
ash 0xcc -4 => 0xfc | 612
ash 0xcc -5 => 0xfe | 612
ash 0xcc -6 => 0xff | 612

; shift count is limited to 65535 bits
ash 0xcc -214783648 => FAIL
ash 0xcc -214783647 => FAIL
ash 0xcc 214783648 => FAIL
ash 0xcc 214783647 => FAIL
ash 0xcc 65536 => FAIL
ash 0xcc -65536 => FAIL
ash 0xcc 256 => 0xcc0000000000000000000000000000000000000000000000000000000000000000 | 1028
ash 0xcc 255 => 0xe60000000000000000000000000000000000000000000000000000000000000000 | 1028
ash 0xcc -256 => 0xff | 612

; parameter isn't allowed to be wider than 32 bits
ash 0xcc 0x0000000001 => FAIL
ash 0xcc "foo" => FAIL

lsh ( 1 2 3 ) 1 => FAIL
lsh 0xffff ( 1 2 3 ) => FAIL
lsh => FAIL
lsh 0xff => FAIL
lsh 0xff 1 => 0x01fe | 306
lsh 0xff 1 1 => FAIL

lsh 0xff -1 => 0x7f | 293
lsh 0x80 -1 => 0x40 | 293
lsh 0x80 -2 => 0x20 | 293
lsh 0x80 -3 => 0x10 | 293
lsh 0x80 -4 => 0x08 | 293
lsh 0x80 -5 => 0x04 | 293
lsh 0x80 -6 => 0x02 | 293
lsh 0x80 -7 => 0x01 | 293
lsh 0x80 -8 => 0 | 280

lsh 0x7f -1 => 0x3f | 293
lsh 0x7f -2 => 0x1f | 293
lsh 0x7f -3 => 0x0f | 293
lsh 0x7f -4 => 0x07 | 293
lsh 0x7f -5 => 0x03 | 293
lsh 0x7f -6 => 0x01 | 293
lsh 0x7f -7 => 0 | 280
lsh 0x7f -8 => 0 | 280

lsh 0x80 1 => 0x0100 | 306
lsh 0x80 2 => 0x0200 | 306
lsh 0x80 3 => 0x0400 | 306
lsh 0x80 4 => 0x0800 | 306
lsh 0x80 5 => 0x1000 | 306
lsh 0x80 6 => 0x2000 | 306
lsh 0x80 7 => 0x4000 | 306
lsh 0x80 8 => 0x008000 | 316

lsh 0x7f 1 => 0x00fe | 303
lsh 0x7f 2 => 0x01fc | 306
lsh 0x7f 3 => 0x03f8 | 306
lsh 0x7f 4 => 0x07f0 | 306
lsh 0x7f 5 => 0x0fe0 | 306
lsh 0x7f 6 => 0x1fc0 | 306
lsh 0x7f 7 => 0x3f80 | 306
lsh 0x7f 8 => 0x7f00 | 306

lsh 0x90000000000000000000000000 -100 => 0x09 | 329
lsh 0xf9 100 => 0x0f90000000000000000000000000 | 462
lsh 0xcc 1 => 0x0198 | 306
lsh 0xcc 2 => 0x0330 | 306
lsh 0xcc 3 => 0x0660 | 306
lsh 0xcc 4 => 0x0cc0 | 306
lsh 0xcc 5 => 0x1980 | 306
lsh 0xcc 6 => 0x3300 | 306

lsh 0xcc -1 => 0x66 | 293
lsh 0xcc -2 => 0x33 | 293
lsh 0xcc -3 => 0x19 | 293
lsh 0xcc -4 => 0x0c | 293
lsh 0xcc -5 => 0x06 | 293
lsh 0xcc -6 => 0x03 | 293

; shift count is limited to 65535 bits
lsh 0xcc -214783648 => FAIL
lsh 0xcc -214783647 => FAIL
lsh 0xcc 214783648 => FAIL
lsh 0xcc 214783647 => FAIL
lsh 0xcc 65536 => FAIL
lsh 0xcc -65536 => FAIL
lsh 0xcc 256 => 0x00cc0000000000000000000000000000000000000000000000000000000000000000 | 719
lsh 0xcc 255 => 0x660000000000000000000000000000000000000000000000000000000000000000 | 709
lsh 0xcc -256 => 0 | 280

; parameter isn't allowed to be wider than 32 bits
lsh 0xcc 0x0000000001 => FAIL
lsh 0xcc "foo" => FAIL

not => FAIL
not 1 2 => FAIL
not 0 => 1 | 200
not 1 => 0 | 200
not 0xffff => 0 | 200
; a sigle zero-byte counts as true
not 0x00 => 0 | 200

; a non-empty list counts as "true"
not ( 1 2 3 ) => 0 | 200
not ( ) => 1 | 200

any => 0 | 200
any 0 => 0 | 500
any 1 0 => 1 | 800
any 0 1 => 1 | 800
; a sigle zero-byte counts as true
any 0x00 => 1 | 500

; a non-empty list counts as "true"
any 0 ( 1 2 ) => 1 | 800
any ( ) ( 1 2 ) => 1 | 800
any ( ) 0 => 0 | 800

all => 1 | 200
all 0 => 0 | 500
all 1 0 => 0 | 800
all 0 1 => 0 | 800
all 1 2 3 => 1 | 1100
all 0x00 => 1 | 500
all 0x00 0 => 0 | 800

; a non-empty list counts as "true"
all ( 1 ) 2 3 => 1 | 1100
all ( 1 ) 2 ( ) => 0 | 1100

x => FAIL
x ( "msg" ) => FAIL
x "error_message" => FAIL

> => FAIL
> 0 => FAIL
> 0 0 => 0 | 498

> ( 1 0 ) => FAIL
> ( 1 ) 0 => FAIL
> 0 ( 1 ) => FAIL
> 1 0 => 1 | 500
> 0 1 => 0 | 500
> 0 -1 => 1 | 500
> -1 0 => 0 | 500
> 0x0000000000000000000000000000000000000000000000000000000000000000000493e0 0x000000000000000000000000000000000000000000000000000000000000005a => 1 | 634
> 3 300 => 0 | 504
> 300 3 => 1 | 504
> "foobar" "foo" => 1 | 516
> "foo" "boo" => 1 | 510
> "bar" "foo" => 0 | 510

>s => FAIL
>s 0x00 => FAIL
>s 0x00 0x00 => 0 | 119
>s 0x00 0x00 0x00 => FAIL
>s ( 1 ) ( 2 ) => FAIL
>s "foo" ( 2 ) => FAIL
>s ( 2 ) "foo" => FAIL

; -1 is 0xff which compares greater than 0, an empty atom
>s -1 0 => 1 | 118
>s 0 -1 => 0 | 118
>s 0x01 0x00 => 1 | 119
>s 0x1001 0x1000 => 1 | 121
>s 0x1000 0x1001 => 0 | 121
>s "foo" "bar" => 1 | 123
>s "bar" "foo" => 0 | 123
>s "foo" "foo" => 0 | 123

= => FAIL
= 0x00 => FAIL
= 0x00 0x00 0x00 => FAIL
= ( "foo" ) "foo" => FAIL
= "foo" ( "foo" ) => FAIL

= 0 0 => 1 | 117
= 1 1 => 1 | 119
= 0 0 => 1 | 117
= 0 0x00 => 0 | 118
= 0x00 0 => 0 | 118
= 0xff 0xffff => 0 | 120
= -1 -1 => 1 | 119
= 1 1 => 1 | 119
= 256 256 => 1 | 121
= 255 -1 => 0 | 120
= 65535 -1 => 0 | 121
= 65535 65535 => 1 | 123
= 65536 65536 => 1 | 123
= 4294967295 4294967295 => 1 | 127
= 4294967296 4294967296 => 1 | 127
= 2147483647 2147483647 => 1 | 125
= 2147483648 2147483648 => 1 | 127
= 0x00000000000000000000000000000000000000000000000000000010 0x00000000000000000000000000000000000000000000000000000010 => 1 | 173
= 0x00000000000000000000000000000000000000000000000000000010 0x00000000000000000000000000000000000000000000000000000020 => 0 | 173

+ ( 1 ) => FAIL
+ 1 ( 2 ) => FAIL
+ ( 2 ) 1 => FAIL
+ => 0 | 99
+ 0 => 0 | 419
+ 0 0 => 0 | 739
+ 0 0 0 => 0 | 1059
+ -1 1 => 0 | 745
+ -100 100 => 0 | 745
+ 100 -100 => 0 | 745
+ 32768 32768 => 65536 | 787
+ -32768 -32768 => -65536 | 781
+ 65536 65536 => 131072 | 787
+ -65536 -65536 => -131072 | 787
+ -32768 -32768 => -65536 | 781
+ 2147483648 2147483648 => 4294967296 | 819
+ -2147483648 -2147483648 => -4294967296 | 813
+ 0x010000000000000000 0x010000000000000000 => 0x020000000000000000 | 883
+ 18446744073709551616 18446744073709551616 => 36893488147419103232 | 883
+ -18446744073709551616 -18446744073709551616 => -36893488147419103232 | 883
+ 18446744073709551616 -18446744073709551616 => 0 | 793
+ -18446744073709551616 18446744073709551616 => 0 | 793
+ 0x00cccccccc 0x33333333 => 0x00ffffffff | 816
; -3355444 + 3355443 = -1
+ 0xcccccccc 0x33333333 => 0xff | 773
+ 0x00000000000000000000000000000003 0x00000000000000000000000000000002 => 5 | 845

- ( 1 ) => FAIL
- 1 ( 1 ) => FAIL
- ( 2 ) 2 => FAIL
- => 0 | 99
- 0 => 0 | 419
- 0 0 => 0 | 739
- 0 0 0 => 0 | 1059
- -1 1 => -2 | 755
- 1 -1 => 2 | 755
- -100 100 => -200 | 765
- 100 -100 => 200 | 765
- 32768 32768 => 0 | 757
- -32768 -32768 => 0 | 751
- 32768 -32768 => 65536 | 784
- 65536 65536 => 0 | 757
- 65536 -65536 => 131072 | 787
- -65536 -65536 => 0 | 757
- -32768 -32768 => 0 | 751
- 2147483648 2147483648 => 0 | 769
- 2147483648 -2147483648 => 4294967296 | 816
- -2147483648 -2147483648 => 0 | 763
- 0x010000000000000000 0x010000000000000000 => 0 | 793
- 18446744073709551616 -18446744073709551616 => 36893488147419103232 | 883
- 0 18446744073709551616 18446744073709551616 => -36893488147419103232 | 1203
- -18446744073709551616 -18446744073709551616 => 0 | 793
- 18446744073709551616 18446744073709551616 => 0 | 793
- 0x00cccccccc 0x33333333 => 0x0099999999 | 816
; -3355444 - 3355443 = -6710887
- 0xcccccccc 0x33333333 => 0x99999999 | 803
- 0x00000000000000000000000000000003 0x00000000000000000000000000000002 => 1 | 845
- 0 35768 => -35768 | 778
- 0 65536 => -65536 | 778
- 0 2147483648 => -2147483648 | 794
- 0 4294967296 => -4294967296 | 804
- 0 18446744073709551616 => -18446744073709551616 | 856

* ( 2 ) => FAIL
* 1 ( 2 ) => FAIL
* ( 2 ) 1 => FAIL
* => 1 | 102
* 0 => 0 | 92
* "foobar" => "foobar" | 152
* 1337 => 1337 | 112
* 7 2 => 14 | 999
* 2 2 2 2 2 2 2 2 => 256 | 6391
* 10 10 10 10 => 10000 | 2809
* 7 -1 => -7 | 999
* -1 7 => -7 | 999
* 1337 -1 => -1337 | 1015
* -1 1337 => -1337 | 1015
* -1 -1 => 1 | 999
* -1 1 => -1 | 999
* 1 -1 => -1 | 999
* 1 1 => 1 | 999
* -1 -1 -1 -1 -1 -1 -1 -1 => 1 | 6381
* -1 -1 -1 -1 -1 -1 -1 -1 -1 => -1 | 7278
* 0x000000000000000007 0x000000000000000002 => 14 | 1095
* 0x000000000000000007 0xffffffffffffffffff => -7 | 1095
* 0x000000000000000007 0xffffffffffffffffff 0 => 0 | 1976
* 0x010000 0x010000 => 0x0100000000 | 1063
* 0x0100000000 0x0100000000 => 0x010000000000000000 | 1127
* 4294967296 4294967296 4294967296 => 79228162514264337593543950336 | 2136
* 4294967296 4294967296 -4294967296 => -79228162514264337593543950336 | 2136
* 4294967296 -4294967296 -4294967296 => 79228162514264337593543950336 | 2136
* 65536 65536 65536 => 281474976710656 | 2016
* 65536 65536 -65536 => -281474976710656 | 2016
* 65536 -65536 -65536 => 281474976710656 | 2016
* 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 => 10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | 14198

; wrong number of arguments
/ => FAIL
/ 1 => FAIL
/ 1 2 3 => FAIL

; division by zero
/ 0 0 => FAIL
/ 10 0 => FAIL
/ -10 0 => FAIL

; division round towards negative infinity
/ 10 3 => 3 | 1006
/ -10 3 => -4 | 1006
/ -10 -3 => 3 | 1006
/ 10 -3 => -4 | 1006

/ 80001 73 => 1095 | 1024
/ -80001 73 => -1096 | 1024
/ 0x00000000000000000a 0x000000000000000005 => 2 | 1070

/ 1 10 => 0 | 996
/ -1 -10 => 0 | 996

/ 1 1 => 1 | 1006
/ 1 -1 => -1 | 1006
/ -1 -1 => 1 | 1006
/ -1 1 => -1 | 1006
/ 0 -1 => 0 | 992
/ 0 1 => 0 | 992

; these results are incorrect.
; the result should be -1
; the / operator is deprecated because of this
/ -1 10 => 0 | 996
/ 1 -10 => 0 | 996

; the mempool version of op_div disallows negative operands

; wrong number of arguments
div_depr => FAIL
div_depr 1 => FAIL
div_depr 1 2 3 => FAIL

; division by zero
div_depr 0 0 => FAIL
div_depr 10 0 => FAIL
div_depr -10 0 => FAIL

; division round towards negative infinity
div_depr 10 3 => 3 | 1006
div_depr -10 3 => FAIL
div_depr -10 -3 => FAIL
div_depr 10 -3 => FAIL

div_depr 80001 73 => 1095 | 1024
div_depr -80001 73 => FAIL
div_depr 0x00000000000000000a 0x000000000000000005 => 2 | 1070

div_depr 1 10 => 0 | 996
div_depr -1 -10 => FAIL

div_depr 1 1 => 1 | 1006
div_depr 1 -1 =>  FAIL
div_depr -1 -1 => FAIL
div_depr -1 1 => FAIL
div_depr 0 -1 => FAIL
div_depr 0 1 => 0 | 992

; these results are incorrect.
; the result should be -1
; the div_depr operator is deprecated because of this
div_depr -1 10 => FAIL
div_depr 1 -10 => FAIL

; wrong number of arguments
divmod => FAIL
divmod ( 2 ) => FAIL
divmod 1 => FAIL
divmod 1 ( 2 ) => FAIL
divmod ( 2 ) 1 => FAIL
divmod 1 2 3 => FAIL

; division by zero
divmod 0 0 => FAIL
divmod 10 0 => FAIL
divmod -10 0 => FAIL

; division round towards negative infinity
divmod 10 3 => ( 3 . 1 ) | 1148
divmod -10 3 => ( -4 . 2 ) | 1148
divmod -10 -3 => ( 3 . -1 ) | 1148
divmod 10 -3 => ( -4 . -2 ) | 1148

divmod 1 3 => ( 0 . 1 ) | 1138
divmod -1 3 => ( -1 . 2 ) | 1148
divmod -1 -3 => ( 0 . -1 ) | 1138
divmod 1 -3 => ( -1 . -2 ) | 1148

divmod 1 1 => ( 1 . 0 ) | 1138
divmod -1 1 => ( -1 . 0 ) | 1138
divmod -1 -1 => ( 1 . 0 ) | 1138
divmod 1 -1 => ( -1 . 0 ) | 1138

divmod 1 10 => ( 0 . 1 ) | 1138
divmod 1 -10 => ( -1 . -9 ) | 1148
divmod -1 -10 => ( 0 . -1 ) | 1138
divmod -1 10 => ( -1 . 9 ) | 1148
divmod 1 -1000000000000 => ( -1 . -999999999999 ) | 1228
divmod -1 1000000000000 => ( -1 . 999999999999 ) | 1228

divmod 80001 73 => ( 1095 . 66 ) | 1170
divmod -80001 73 => ( -1096 . 7 ) | 1170
divmod 0x00000000000000000a 0x000000000000000005 => ( 2 . 0 ) | 1234

; cost argument is required
softfork => FAIL

; cost must be an integer
softfork ( 50 ) => FAIL

; cost must be > 0
softfork 0 => FAIL
softfork -1 => FAIL

softfork 50 => 0 | 50
softfork 51 110 => 0 | 51
softfork => FAIL
softfork 3121 => 0 | 3121
softfork 0x00000000000000000000000000000000000050 => 0 | 80
softfork 0xffffffffffffffff => FAIL
; technically, this is a valid cost, but it still exceeds the limit we set for the tests
softfork 0xffffffffffffff => FAIL
softfork 0 => FAIL

strlen => FAIL
strlen ( "list" ) => FAIL
strlen "" => 0 | 173
strlen "a" => 1 | 184
strlen "foobar" => 6 | 189
; this is just 0xff
strlen -1 => 1 | 184

concat ( "ab" ) => FAIL
concat ( "ab" ) "1" => FAIL
concat => "" | 142
concat "" => "" | 277
concat "a" => "a" | 290
concat "a" "b" => "ab" | 438
concat "a" "b" "c" => "abc" | 586
concat "abc" "" => "abc" | 451
concat "" "ab" "c" => "abc" | 586
concat 0xff 0x00 => 0xff00 | 438
concat 0x00 0x00 => 0x0000 | 438
concat 0x00 0xff => 0x00ff | 438
concat 0xbaad 0xf00d => 0xbaadf00d | 464

substr => FAIL
substr "abc" => FAIL
substr ( "abc" ) 1 => FAIL
substr "abc" 1 => "bc" | 1
substr "foobar" 1 => "oobar" | 1
substr "foobar" 1 1 => "" | 1
substr "foobar" 1 2 => "o" | 1
substr "foobar" 3 => "bar" | 1
substr "foobar" 3 4 => "b" | 1
substr "foobar" 1 1 => "" | 1
substr 0x112233445566778899aabbccddeeff 1 2 => 0x22 | 1
substr 0x112233445566778899aabbccddeeff 5 7 => 0x6677 | 1

; one-past-end is a valid index
substr "foobar" 6 6 => "" | 1

; begin must be >= end
substr "foobar" 1 0 => FAIL
substr "foobar" 3 1 => FAIL
substr "foobar" 6 0 => FAIL

; begin must be a valid index
substr "foobar" -1 0 => FAIL
substr "foobar" -1 1 => FAIL
substr "foobar" 7 0 => FAIL
substr "foobar" ( 0 ) 1 => FAIL

; end must be a valid index
substr "foobar" 2 10 => FAIL
substr "foobar" 2 -10 => FAIL
substr "foobar" 5 7 => FAIL
substr "foobar" 0 ( 1 ) => FAIL

; indices must not be narrower than 32 bits
substr "foobar" 0x0000000001 2 => FAIL
substr "foobar" 2 0x0000000003 => FAIL
substr "foobar" 2 0x00000003 => "o" | 1
substr "foobar" 0x00000002 3 => "o" | 1

substr "foobar" 0xffffffff 0xffffffff => FAIL
substr "foobar" 0x00ffffffff 0x00ffffffff => FAIL
substr "foobar" 0x7fffffff 0x7fffffff => FAIL
substr "foobar" 0xffffffff => FAIL
substr "foobar" 0x00ffffffff => FAIL
substr "foobar" 0x7fffffff => FAIL

i ( ) => FAIL
i ( 1 ) => FAIL
i => FAIL
i 1 => FAIL
i 1 1 => FAIL
i 1 1 1 1 => FAIL
i 1 "true" "false" => "true" | 33
i 0 "true" "false" => "false" | 33
i 0 "true" "false" => "false" | 33
i "" "true" "false" => "false" | 33
i 10 "true" "false" => "true" | 33
i -1 "true" "false" => "true" | 33

sha256 ( "foo" "bar" ) => FAIL
sha256 "hello.there.my.dear.friend" => 0x5272821c151fdd49f19cc58cf8833da5781c7478a36d500e8dc2364be39f8216 | 593
sha256 "hell" "o.there.my.dear.friend" => 0x5272821c151fdd49f19cc58cf8833da5781c7478a36d500e8dc2364be39f8216 | 727
; test vectors from https://www.di-mgt.com.au/sha_testvectors.html
sha256 0x616263 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad | 547
sha256 0x61 0x62 0x63 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad | 815
sha256 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 | 407
sha256 "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq" => 0x248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1 | 653

c => FAIL
c 1 => FAIL
c 1 ( 2 ) "garbage" => FAIL
c 1 ( 2 ) => ( 1 2 ) | 50
c 0 ( 2 ) => ( 0 2 ) | 50
c 1 2 => ( 1 . 2 ) | 50
c 1 ( 2 3 4 ) => ( 1 2 3 4 ) | 50
c ( 1 2 3 ) ( 4 5 6 ) => ( ( 1 2 3 ) 4 5 6 ) | 50

f 0 => FAIL
f 1 => FAIL
f ( 1 2 3 ) 1 => FAIL
f ( 1 2 3 ) => 1 | 30
f ( ( 1 2 ) 3 ) => ( 1 2 ) | 30

r 1 => FAIL
r => FAIL
r ( 1 2 3 ) 12 => FAIL
r 0 => FAIL
r ( 1 2 3 ) => ( 2 3 ) | 30
r ( 1 . 2 ) => 2 | 30

l => FAIL
l ( 1 2 ) 1 => FAIL
l ( 1 2 3 ) => 1 | 19
l 1 => 0 | 19
l 0 => 0 | 19
l ( 0 . 0 ) => 1 | 19
l ( 1 . 2 ) => 1 | 19

point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e => 0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224 | 2789534
point_add => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | 101574
; the point must be 40 bytes
point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6 => FAIL
point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb00 => FAIL
point_add 0 => FAIL

; the point must be an atom
point_add ( 1 2 3 ) => FAIL

pubkey_for_exp 1 => 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb | 1326248
pubkey_for_exp 2 => 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e | 1326248
pubkey_for_exp 3 => 0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224 | 1326248
pubkey_for_exp 5 => 0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc | 1326248

pubkey_for_exp -1 => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb | 1326248
pubkey_for_exp -2 => 0x8572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e | 1326248
pubkey_for_exp -3 => 0xa9ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224 | 1326248
pubkey_for_exp -5 => 0x90e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc | 1326248

; This is GROUP_ORDER (and surroundings)
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000002 => 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb | 1327426
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001 => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | 1327426
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000 => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb | 1327426
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00f00000 => 0xb88845f6b070026e15fa44490ad925348ce445eaf4e8bc907cbfab30c5474d20f10f56a18fd0f25f2e18c33fba11d6ce | 1327426

; This is -GROUP_ORDER (and surroundings)
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000fffffffe => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb | 1327426
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000ffffffff => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | 1327426
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a4010000000000000000 => 0x847f5fcce0b9aa0f2bb3de6847337c9ed1bc2184a125c232721e1c81b0f0fee78506790a78c98abff2dd4b01a0756352 | 1327426
"#;

fn parse_atom(a: &mut Allocator, v: &str) -> NodePtr {
    if v == "0" {
        return a.null();
    }

    assert!(v.len() > 0);

    if v.starts_with("0x") {
        let buf = Vec::from_hex(v.strip_prefix("0x").unwrap()).unwrap();
        return a.new_atom(&buf).unwrap();
    }

    if v.starts_with("\"") {
        assert!(v.ends_with("\""));
        let buf = v
            .strip_prefix("\"")
            .unwrap()
            .strip_suffix("\"")
            .unwrap()
            .as_bytes();
        return a.new_atom(&buf).unwrap();
    }

    if v.starts_with("-") || "0123456789".contains(v.get(0..1).unwrap()) {
        let num = Number::from_str_radix(v, 10).unwrap();
        return ptr_from_number(a, &num).unwrap();
    }

    panic!("atom not supported \"{}\"", v);
}

fn pop_token<'a>(s: &'a str) -> (&'a str, &'a str) {
    match s.split_once(" ") {
        Some((first, rest)) => (first.trim(), rest.trim()),
        None => (s.trim(), ""),
    }
}

fn parse_list<'a>(a: &mut Allocator, v: &'a str) -> (NodePtr, &'a str) {
    let v = v.trim();
    let (first, rest) = pop_token(v);
    if first.len() == 0 {
        return (a.null(), rest);
    }
    if first == ")" {
        return (a.null(), rest);
    }
    if first == "(" {
        let (head, new_rest) = parse_list(a, rest);
        let (tail, new_rest) = parse_list(a, new_rest);
        (a.new_pair(head, tail).unwrap(), new_rest)
    } else if first == "." {
        let (first, new_rest) = pop_token(rest);
        let node = parse_atom(a, first);
        let (end_list, new_rest) = pop_token(new_rest);
        assert_eq!(end_list, ")");
        (node, new_rest)
    } else {
        let head = parse_atom(a, first);
        let (tail, new_rest) = parse_list(a, rest);
        (a.new_pair(head, tail).unwrap(), new_rest)
    }
}

fn node_eq(a: &Allocator, a0: NodePtr, a1: NodePtr) -> bool {
    match a.sexp(a0) {
        SExp::Pair(left0, right0) => {
            if let SExp::Pair(left1, right1) = a.sexp(a1) {
                node_eq(a, left0, left1) && node_eq(a, right0, right1)
            } else {
                false
            }
        }
        SExp::Atom(_) => {
            if let SExp::Atom(_) = a.sexp(a1) {
                a.atom(a0) == a.atom(a1)
            } else {
                false
            }
        }
    }
}

type Opf = fn(&mut Allocator, NodePtr, Cost) -> Response;

// the input is a list of test cases, each item is a tuple of:
// (function pointer to test, list of arguments, optional result)
// if the result is None, the call is expected to fail
fn run_op_test(op: &Opf, args_str: &str, expected: &str, expected_cost: u64) {
    let mut a = Allocator::new();

    let (args, rest) = parse_list(&mut a, args_str);
    assert_eq!(rest, "");
    let result = op(&mut a, args, 10000000000 as Cost);
    match result {
        Err(_) => {
            assert_eq!(expected, "FAIL");
        }
        Ok(Reduction(cost, ret_value)) => {
            assert_eq!(cost, expected_cost);
            if expected.starts_with("(") {
                let (expected, rest) = parse_list(&mut a, expected.get(1..).unwrap());
                assert_eq!(rest, "");
                assert!(node_eq(&a, ret_value, expected));
            } else {
                let expected = parse_atom(&mut a, expected);
                assert_eq!(a.atom(ret_value), a.atom(expected));
            }
        }
    }
}

#[test]
fn test_ops() {
    let funs = HashMap::from([
        ("i", op_if as Opf),
        ("c", op_cons as Opf),
        ("f", op_first as Opf),
        ("r", op_rest as Opf),
        ("l", op_listp as Opf),
        ("x", op_raise as Opf),
        ("=", op_eq as Opf),
        ("sha256", op_sha256 as Opf),
        ("+", op_add as Opf),
        ("-", op_subtract as Opf),
        ("*", op_multiply as Opf),
        ("/", op_div as Opf),
        ("div_depr", op_div_deprecated as Opf),
        ("divmod", op_divmod as Opf),
        ("substr", op_substr as Opf),
        ("strlen", op_strlen as Opf),
        ("point_add", op_point_add as Opf),
        ("pubkey_for_exp", op_pubkey_for_exp as Opf),
        ("concat", op_concat as Opf),
        (">", op_gr as Opf),
        (">s", op_gr_bytes as Opf),
        ("logand", op_logand as Opf),
        ("logior", op_logior as Opf),
        ("logxor", op_logxor as Opf),
        ("lognot", op_lognot as Opf),
        ("ash", op_ash as Opf),
        ("lsh", op_lsh as Opf),
        ("not", op_not as Opf),
        ("any", op_any as Opf),
        ("all", op_all as Opf),
        ("softfork", op_softfork as Opf),
    ]);

    for t in TEST_CASES.split("\n") {
        let t = t.trim();
        if t.len() == 0 {
            continue;
        }
        // ignore comments
        if t.starts_with(";") {
            continue;
        }
        let (op_name, t) = t.split_once(" ").unwrap();
        let op = funs.get(op_name).unwrap();
        let (args, out) = t.split_once("=>").unwrap();
        let (expected, expected_cost) = if out.contains("|") {
            out.split_once("|").unwrap()
        } else {
            (out, "0")
        };

        println!("({} {}) => {}", op_name, args.trim(), expected.trim());
        run_op_test(
            op,
            args.trim(),
            expected.trim(),
            expected_cost.trim().parse().unwrap(),
        );
    }
}
