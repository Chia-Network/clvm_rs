use crate::allocator::{Allocator, NodePtr, SExp};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_div_deprecated, op_divmod, op_gr,
    op_gr_bytes, op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not,
    op_point_add, op_pubkey_for_exp, op_sha256, op_softfork, op_strlen, op_substr, op_subtract,
};
use crate::number::{ptr_from_number, Number};
use crate::number_traits::TestNumberTraits;
use crate::reduction::{Reduction, Response};
use hex::FromHex;
use std::collections::HashMap;

static TEST_CASES: &str = r#"
lognot ( 1 2 3 ) => FAIL
lognot 0xff => 0
lognot 0xffffff => 0
lognot 0x0000ff => 0xff00
lognot 0x0000000001 => 0xfe
lognot 0xff00 => 0x00ff
lognot 0x0c => 0xf3
lognot 0 => 0xff
lognot 0xcccccc => 0x333333
lognot 0x333333 => 0xcccccc
; requires exactly one argument
lognot 0x00 0x00 => FAIL
lognot => FAIL

logior ( 1 2 3 ) => FAIL
logior => 0
logior 0xbaadf00d => 0xbaadf00d
logior 0xf0 0x0f => 0xff
logior 0xcc 0x33 => 0xff
logior 0x800000 0x01 => 0x800001
logior 0x400000 0x01 => 0x400001
logior 0x000040 0x01 => 0x41
logior 0x000080 0x01 => 0x0081
logior 0x000080 0xff => 0xff
logior 0x000070 0xff => 0xff
logior 0x000080 0x7f => 0x00ff
logior 0xffff80 0x01 => 0x81
logior 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0xff
logior 0x01 0x01 => 0x01
logior 0x01 0x01 0x01 => 0x01

logxor ( 1 2 3 ) => FAIL
logxor => 0
logxor 0xbaadf00d => 0xbaadf00d
logxor 0xf0 0x0f => 0xff
logxor 0xcc 0x33 => 0xff
logxor 0x800000 0x01 => 0x800001
logxor 0x400000 0x01 => 0x400001
logxor 0x000040 0x01 => 0x41
logxor 0x000080 0x01 => 0x0081
logxor 0x000080 0xff => 0xff7f
logxor 0x000080 0x7f => 0x00ff
logxor 0x000070 0xff => 0x8f
logxor 0xffff80 0x01 => 0x81
logxor 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0xff
logxor 0x01 0x01 => 0
logxor 0x01 0x01 0x01 => 0x01

logand ( 1 2 3 ) => FAIL
logand => 0xff
logand 0xbaadf00d => 0xbaadf00d
logand 0xf0 0x0f => 0
logand 0xcc 0x33 => 0
logand 0x800000 0x01 => 0
logand 0x400000 0x01 => 0
logand 0x000040 0x01 => 0
logand 0x000080 0x01 => 0
; 0x000080 -> 0x0080, 0xff -> 0xffff
logand 0x000080 0xff => 0x0080
logand 0x000040 0xff => 0x40
logand 0x000080 0x7f => 0
logand 0x000070 0xff => 0x70
logand 0xffff80 0x01 => 0
logand 0x01 0x02 0x04 0x08 0x10 0x20 0x40 0x80 => 0
logand 0x01 0x01 => 0x01
logand 0x01 0x01 0x01 => 0x01

ash => FAIL
ash ( 1 2 3 ) 1 => FAIL
ash 0xffff ( 1 2 3 ) => FAIL
ash 0xff => FAIL
ash 0xff 1 => 0xfe
ash 0xff 1 1 => FAIL

ash 0xff -1 => 0xff
ash 0x80 -1 => 0xc0
ash 0x80 -2 => 0xe0
ash 0x80 -3 => 0xf0
ash 0x80 -4 => 0xf8
ash 0x80 -5 => 0xfc
ash 0x80 -6 => 0xfe
ash 0x80 -7 => 0xff
ash 0x80 -8 => 0xff

ash 0x7f -1 => 0x3f
ash 0x7f -2 => 0x1f
ash 0x7f -3 => 0x0f
ash 0x7f -4 => 0x07
ash 0x7f -5 => 0x03
ash 0x7f -6 => 0x01
ash 0x7f -7 => 0
ash 0x7f -8 => 0

ash 0x80 1 => 0xff00
ash 0x80 2 => 0xfe00
ash 0x80 3 => 0xfc00
ash 0x80 4 => 0xf800
ash 0x80 5 => 0xf000
ash 0x80 6 => 0xe000
ash 0x80 7 => 0xc000
ash 0x80 8 => 0x8000

ash 0x7f 1 => 0x00fe
ash 0x7f 2 => 0x01fc
ash 0x7f 3 => 0x03f8
ash 0x7f 4 => 0x07f0
ash 0x7f 5 => 0x0fe0
ash 0x7f 6 => 0x1fc0
ash 0x7f 7 => 0x3f80
ash 0x7f 8 => 0x7f00

ash 0x90000000000000000000000000 -100 => -7
ash -7 100 => 0x90000000000000000000000000
ash 0xcc 1 => 0x98
ash 0xcc 2 => 0xff30
ash 0xcc 3 => 0xfe60
ash 0xcc 4 => 0xfcc0
ash 0xcc 5 => 0xf980
ash 0xcc 6 => 0xf300

ash 0xcc -1 => 0xe6
ash 0xcc -2 => 0xf3
ash 0xcc -3 => 0xf9
ash 0xcc -4 => 0xfc
ash 0xcc -5 => 0xfe
ash 0xcc -6 => 0xff

; shift count is limited to 65535 bits
ash 0xcc -214783648 => FAIL
ash 0xcc -214783647 => FAIL
ash 0xcc 214783648 => FAIL
ash 0xcc 214783647 => FAIL
ash 0xcc 65536 => FAIL
ash 0xcc -65536 => FAIL
ash 0xcc 256 => 0xcc0000000000000000000000000000000000000000000000000000000000000000
ash 0xcc 255 => 0xe60000000000000000000000000000000000000000000000000000000000000000
ash 0xcc -256 => 0xff

; parameter isn't allowed to be wider than 32 bits
ash 0xcc 0x0000000001 => FAIL
ash 0xcc "foo" => FAIL

lsh ( 1 2 3 ) 1 => FAIL
lsh 0xffff ( 1 2 3 ) => FAIL
lsh => FAIL
lsh 0xff => FAIL
lsh 0xff 1 => 0x01fe
lsh 0xff 1 1 => FAIL

lsh 0xff -1 => 0x7f
lsh 0x80 -1 => 0x40
lsh 0x80 -2 => 0x20
lsh 0x80 -3 => 0x10
lsh 0x80 -4 => 0x08
lsh 0x80 -5 => 0x04
lsh 0x80 -6 => 0x02
lsh 0x80 -7 => 0x01
lsh 0x80 -8 => 0

lsh 0x7f -1 => 0x3f
lsh 0x7f -2 => 0x1f
lsh 0x7f -3 => 0x0f
lsh 0x7f -4 => 0x07
lsh 0x7f -5 => 0x03
lsh 0x7f -6 => 0x01
lsh 0x7f -7 => 0
lsh 0x7f -8 => 0

lsh 0x80 1 => 0x0100
lsh 0x80 2 => 0x0200
lsh 0x80 3 => 0x0400
lsh 0x80 4 => 0x0800
lsh 0x80 5 => 0x1000
lsh 0x80 6 => 0x2000
lsh 0x80 7 => 0x4000
lsh 0x80 8 => 0x008000

lsh 0x7f 1 => 0x00fe
lsh 0x7f 2 => 0x01fc
lsh 0x7f 3 => 0x03f8
lsh 0x7f 4 => 0x07f0
lsh 0x7f 5 => 0x0fe0
lsh 0x7f 6 => 0x1fc0
lsh 0x7f 7 => 0x3f80
lsh 0x7f 8 => 0x7f00

lsh 0x90000000000000000000000000 -100 => 0x09
lsh 0xf9 100 => 0x0f90000000000000000000000000
lsh 0xcc 1 => 0x0198
lsh 0xcc 2 => 0x0330
lsh 0xcc 3 => 0x0660
lsh 0xcc 4 => 0x0cc0
lsh 0xcc 5 => 0x1980
lsh 0xcc 6 => 0x3300

lsh 0xcc -1 => 0x66
lsh 0xcc -2 => 0x33
lsh 0xcc -3 => 0x19
lsh 0xcc -4 => 0x0c
lsh 0xcc -5 => 0x06
lsh 0xcc -6 => 0x03

; shift count is limited to 65535 bits
lsh 0xcc -214783648 => FAIL
lsh 0xcc -214783647 => FAIL
lsh 0xcc 214783648 => FAIL
lsh 0xcc 214783647 => FAIL
lsh 0xcc 65536 => FAIL
lsh 0xcc -65536 => FAIL
lsh 0xcc 256 => 0x00cc0000000000000000000000000000000000000000000000000000000000000000
lsh 0xcc 255 => 0x660000000000000000000000000000000000000000000000000000000000000000
lsh 0xcc -256 => 0

; parameter isn't allowed to be wider than 32 bits
lsh 0xcc 0x0000000001 => FAIL
lsh 0xcc "foo" => FAIL

not => FAIL
not 1 2 => FAIL
not 0 => 1
not 1 => 0
not 0xffff => 0
not 0 => 1
; a sigle zero-byte counts as true
not 0x00 => 0

; a non-empty list counts as "true"
not ( 1 2 3 ) => 0
not ( ) => 1

any => 0
any 0 => 0
any 1 0 => 1
any 0 1 => 1
; a sigle zero-byte counts as true
any 0x00 => 1

; a non-empty list counts as "true"
any 0 ( 1 2 ) => 1
any ( ) ( 1 2 ) => 1
any ( ) 0 => 0

all => 1
all 0 => 0
all 1 0 => 0
all 0 1 => 0
all 1 2 3 => 1
all 0x00 => 1
all 0x00 0 => 0

; a non-empty list counts as "true"
all ( 1 ) 2 3 => 1
all ( 1 ) 2 ( ) => 0

x => FAIL
x ( "msg" ) => FAIL
x "error_message" => FAIL

> => FAIL
> 0 => FAIL
> 0 0 => 0

; 0 and 0 are the same
> 0 0 => 0
> 0 0 => 0

> ( 1 0 ) => FAIL
> ( 1 ) 0 => FAIL
> 0 ( 1 ) => FAIL
> 1 0 => 1
> 0 1 => 0
> 0 -1 => 1
> -1 0 => 0
> 0x0000000000000000000000000000000000000000000000000000000000000000000493e0 0x000000000000000000000000000000000000000000000000000000000000005a => 1
> 3 300 => 0
> 300 3 => 1
> "foobar" "foo" => 1
> "foo" "boo" => 1
> "bar" "foo" => 0

>s => FAIL
>s 0x00 => FAIL
>s 0x00 0x00 => 0
>s 0x00 0x00 0x00 => FAIL
>s ( 1 ) ( 2 ) => FAIL
>s "foo" ( 2 ) => FAIL
>s ( 2 ) "foo" => FAIL

; -1 is 0xff which compares greater than 0, an empty atom
>s -1 0 => 1
>s 0 -1 => 0
>s 0x01 0x00 => 1
>s 0x1001 0x1000 => 1
>s 0x1000 0x1001 => 0
>s "foo" "bar" => 1
>s "bar" "foo" => 0
>s "foo" "foo" => 0

= => FAIL
= 0x00 => FAIL
= 0x00 0x00 0x00 => FAIL
= ( "foo" ) "foo" => FAIL
= "foo" ( "foo" ) => FAIL

= 0 0 => 1
= 1 1 => 1
= 0 0 => 1
= 0 0x00 => 0
= 0x00 0 => 0
= 0xff 0xffff => 0
= -1 -1 => 1
= 1 1 => 1
= 256 256 => 1
= 255 -1 => 0
= 65535 -1 => 0
= 65535 65535 => 1
= 65536 65536 => 1
= 4294967295 4294967295 => 1
= 4294967296 4294967296 => 1
= 2147483647 2147483647 => 1
= 2147483648 2147483648 => 1
= 0x00000000000000000000000000000000000000000000000000000010 0x00000000000000000000000000000000000000000000000000000010 => 1
= 0x00000000000000000000000000000000000000000000000000000010 0x00000000000000000000000000000000000000000000000000000020 => 0

+ ( 1 ) => FAIL
+ 1 ( 2 ) => FAIL
+ ( 2 ) 1 => FAIL
+ => 0
+ 0 => 0
+ 0 0 => 0
+ 0 0 0 => 0
+ -1 1 => 0
+ -100 100 => 0
+ 100 -100 => 0
+ 32768 32768 => 65536
+ -32768 -32768 => -65536
+ 65536 65536 => 131072
+ -65536 -65536 => -131072
+ -32768 -32768 => -65536
+ 2147483648 2147483648 => 4294967296
+ -2147483648 -2147483648 => -4294967296
+ 0x010000000000000000 0x010000000000000000 => 0x020000000000000000
+ 18446744073709551616 18446744073709551616 => 36893488147419103232
+ -18446744073709551616 -18446744073709551616 => -36893488147419103232
+ 18446744073709551616 -18446744073709551616 => 0
+ -18446744073709551616 18446744073709551616 => 0
+ 0x00cccccccc 0x33333333 => 0x00ffffffff
; -3355444 + 3355443 = -1
+ 0xcccccccc 0x33333333 => 0xff
+ 0x00000000000000000000000000000003 0x00000000000000000000000000000002 => 5

- ( 1 ) => FAIL
- 1 ( 1 ) => FAIL
- ( 2 ) 2 => FAIL
- => 0
- 0 => 0
- 0 0 => 0
- 0 0 0 => 0
- -1 1 => -2
- 1 -1 => 2
- -100 100 => -200
- 100 -100 => 200
- 32768 32768 => 0
- -32768 -32768 => 0
- 32768 -32768 => 65536
- 65536 65536 => 0
- 65536 -65536 => 131072
- -65536 -65536 => 0
- -32768 -32768 => 0
- 2147483648 2147483648 => 0
- 2147483648 -2147483648 => 4294967296
- -2147483648 -2147483648 => 0
- 0x010000000000000000 0x010000000000000000 => 0
- 18446744073709551616 -18446744073709551616 => 36893488147419103232
- 0 18446744073709551616 18446744073709551616 => -36893488147419103232
- -18446744073709551616 -18446744073709551616 => 0
- 18446744073709551616 18446744073709551616 => 0
- 0x00cccccccc 0x33333333 => 0x0099999999
; -3355444 - 3355443 = -6710887
- 0xcccccccc 0x33333333 => 0x99999999
- 0x00000000000000000000000000000003 0x00000000000000000000000000000002 => 1
- 0 35768 => -35768
- 0 65536 => -65536
- 0 2147483648 => -2147483648
- 0 4294967296 => -4294967296
- 0 18446744073709551616 => -18446744073709551616

* ( 2 ) => FAIL
* 1 ( 2 ) => FAIL
* ( 2 ) 1 => FAIL
* => 1
* 0 => 0
* "foobar" => "foobar"
* 1337 => 1337
* 7 2 => 14
* 2 2 2 2 2 2 2 2 => 256
* 10 10 10 10 => 10000
* 7 -1 => -7
* -1 7 => -7
* 1337 -1 => -1337
* -1 1337 => -1337
* -1 -1 => 1
* -1 1 => -1
* 1 -1 => -1
* 1 1 => 1
* -1 -1 -1 -1 -1 -1 -1 -1 => 1
* -1 -1 -1 -1 -1 -1 -1 -1 -1 => -1
* 0x000000000000000007 0x000000000000000002 => 14
* 0x000000000000000007 0xffffffffffffffffff => -7
* 0x000000000000000007 0xffffffffffffffffff 0 => 0
* 0x010000 0x010000 => 0x0100000000
* 0x0100000000 0x0100000000 => 0x010000000000000000
* 4294967296 4294967296 4294967296 => 79228162514264337593543950336
* 4294967296 4294967296 -4294967296 => -79228162514264337593543950336
* 4294967296 -4294967296 -4294967296 => 79228162514264337593543950336
* 65536 65536 65536 => 281474976710656
* 65536 65536 -65536 => -281474976710656
* 65536 -65536 -65536 => 281474976710656
* 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 10000000000000000000000000000000000 => 10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

; wrong number of arguments
/ => FAIL
/ 1 => FAIL
/ 1 2 3 => FAIL

; division by zero
/ 0 0 => FAIL
/ 10 0 => FAIL
/ -10 0 => FAIL

; division round towards negative infinity
/ 10 3 => 3
/ -10 3 => -4
/ -10 -3 => 3
/ 10 -3 => -4

/ 80001 73 => 1095
/ -80001 73 => -1096
/ 0x00000000000000000a 0x000000000000000005 => 2

/ 1 10 => 0
/ -1 -10 => 0

/ 1 1 => 1
/ 1 -1 => -1
/ -1 -1 => 1
/ -1 1 => -1
/ 0 -1 => 0
/ 0 1 => 0

; these results are incorrect.
; the result should be -1
; the / operator is deprecated because of this
/ -1 10 => 0
/ 1 -10 => 0

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
div_depr 10 3 => 3
div_depr -10 3 => FAIL
div_depr -10 -3 => FAIL
div_depr 10 -3 => FAIL

div_depr 80001 73 => 1095
div_depr -80001 73 => FAIL
div_depr 0x00000000000000000a 0x000000000000000005 => 2

div_depr 1 10 => 0
div_depr -1 -10 => FAIL

div_depr 1 1 => 1
div_depr 1 -1 =>  FAIL
div_depr -1 -1 => FAIL
div_depr -1 1 => FAIL
div_depr 0 -1 => FAIL
div_depr 0 1 => 0

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
divmod 10 3 => ( 3 . 1 )
divmod -10 3 => ( -4 . 2 )
divmod -10 -3 => ( 3 . -1 )
divmod 10 -3 => ( -4 . -2 )

divmod 1 3 => ( 0 . 1 )
divmod -1 3 => ( -1 . 2 )
divmod -1 -3 => ( 0 . -1 )
divmod 1 -3 => ( -1 . -2 )

divmod 1 1 => ( 1 . 0 )
divmod -1 1 => ( -1 . 0 )
divmod -1 -1 => ( 1 . 0 )
divmod 1 -1 => ( -1 . 0 )

divmod 1 10 => ( 0 . 1 )
divmod 1 -10 => ( -1 . -9 )
divmod -1 -10 => ( 0 . -1 )
divmod -1 10 => ( -1 . 9 )
divmod 1 -1000000000000 => ( -1 . -999999999999 )
divmod -1 1000000000000 => ( -1 . 999999999999 )

divmod 80001 73 => ( 1095 . 66 )
divmod -80001 73 => ( -1096 . 7 )
divmod 0x00000000000000000a 0x000000000000000005 => ( 2 . 0 )

; cost argument is required
softfork => FAIL

; cost must be an integer
softfork ( 50 ) => FAIL

; cost must be > 0
softfork 0 => FAIL
softfork -1 => FAIL

softfork 50 => 0
softfork 51 110 => 0
softfork => FAIL
softfork 3121 => 0
softfork 0x00000000000000000000000000000000000050 => 0
softfork 0xffffffffffffffff => FAIL
; technically, this is a valid cost, but it still exceeds the limit we set for the tests
softfork 0xffffffffffffff => FAIL
softfork 0 => FAIL

strlen => FAIL
strlen ( "list" ) => FAIL
strlen "" => 0
strlen "a" => 1
strlen "foobar" => 6
; this is just 0xff
strlen -1 => 1

concat ( "ab" ) => FAIL
concat ( "ab" ) "1" => FAIL
concat => ""
concat "" => ""
concat "a" => "a"
concat "a" "b" => "ab"
concat "a" "b" "c" => "abc"
concat "abc" "" => "abc"
concat "" "ab" "c" => "abc"
concat 0xff 0x00 => 0xff00
concat 0x00 0x00 => 0x0000
concat 0x00 0xff => 0x00ff
concat 0xbaad 0xf00d => 0xbaadf00d

substr => FAIL
substr "abc" => FAIL
substr ( "abc" ) 1 => FAIL
substr "abc" 1 => "bc"
substr "foobar" 1 => "oobar"
substr "foobar" 1 1 => ""
substr "foobar" 1 2 => "o"
substr "foobar" 3 => "bar"
substr "foobar" 3 4 => "b"
substr "foobar" 1 1 => ""
substr 0x112233445566778899aabbccddeeff 1 2 => 0x22
substr 0x112233445566778899aabbccddeeff 5 7 => 0x6677

; one-past-end is a valid index
substr "foobar" 6 6 => ""

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
substr "foobar" 2 0x00000003 => "o"
substr "foobar" 0x00000002 3 => "o"

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
i 1 "true" "false" => "true"
i 0 "true" "false" => "false"
i 0 "true" "false" => "false"
i "" "true" "false" => "false"
i 10 "true" "false" => "true"
i -1 "true" "false" => "true"

sha256 ( "foo" "bar" ) => FAIL
sha256 "hello.there.my.dear.friend" => 0x5272821c151fdd49f19cc58cf8833da5781c7478a36d500e8dc2364be39f8216
sha256 "hell" "o.there.my.dear.friend" => 0x5272821c151fdd49f19cc58cf8833da5781c7478a36d500e8dc2364be39f8216
; test vectors from https://www.di-mgt.com.au/sha_testvectors.html
sha256 0x616263 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
sha256 0x61 0x62 0x63 => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
sha256 => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
sha256 "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq" => 0x248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1

c => FAIL
c 1 => FAIL
c 1 ( 2 ) "garbage" => FAIL
c 1 ( 2 ) => ( 1 2 )
c 0 ( 2 ) => ( 0 2 )
c 1 2 => ( 1 . 2 )
c 1 ( 2 3 4 ) => ( 1 2 3 4 )
c ( 1 2 3 ) ( 4 5 6 ) => ( ( 1 2 3 ) 4 5 6 )

f 0 => FAIL
f 1 => FAIL
f ( 1 2 3 ) 1 => FAIL
f ( 1 2 3 ) => 1
f ( ( 1 2 ) 3 ) => ( 1 2 )

r 1 => FAIL
r => FAIL
r ( 1 2 3 ) 12 => FAIL
r 0 => FAIL
r ( 1 2 3 ) => ( 2 3 )
r ( 1 . 2 ) => 2

l => FAIL
l ( 1 2 ) 1 => FAIL
l ( 1 2 3 ) => 1
l 1 => 0
l 0 => 0
l ( 0 . 0 ) => 1
l ( 1 . 2 ) => 1

point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e => 0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224
point_add => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
; the point must be 40 bytes
point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6 => FAIL
point_add 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb00 => FAIL
point_add 0 => FAIL

; the point must be an atom
point_add ( 1 2 3 ) => FAIL

pubkey_for_exp 1 => 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb
pubkey_for_exp 2 => 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e
pubkey_for_exp 3 => 0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224
pubkey_for_exp 5 => 0xb0e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc

pubkey_for_exp -1 => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb
pubkey_for_exp -2 => 0x8572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e
pubkey_for_exp -3 => 0xa9ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224
pubkey_for_exp -5 => 0x90e7791fb972fe014159aa33a98622da3cdc98ff707965e536d8636b5fcc5ac7a91a8c46e59a00dca575af0f18fb13dc

; This is GROUP_ORDER (and surroundings)
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000002 => 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001 => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000 => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb
pubkey_for_exp 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00f00000 => 0xb88845f6b070026e15fa44490ad925348ce445eaf4e8bc907cbfab30c5474d20f10f56a18fd0f25f2e18c33fba11d6ce

; This is -GROUP_ORDER (and surroundings)
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000fffffffe => 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a40100000000ffffffff => 0xc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
pubkey_for_exp 0x8c1258acd66282b7ccc627f7f65e27faac425bfd0001a4010000000000000000 => 0x847f5fcce0b9aa0f2bb3de6847337c9ed1bc2184a125c232721e1c81b0f0fee78506790a78c98abff2dd4b01a0756352
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
        let num = Number::from_str_radix(v, 10);
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
fn run_op_test(op: &Opf, args_str: &str, expected: &str) {
    let mut a = Allocator::new();

    let (args, rest) = parse_list(&mut a, args_str);
    assert_eq!(rest, "");
    let result = op(&mut a, args, 10000000000 as Cost);
    match result {
        Err(_) => {
            assert_eq!(expected, "FAIL");
        }
        Ok(Reduction(_cost, ret_value)) => {
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
        let (args, expected) = t.split_once("=>").unwrap();

        println!("({} {}) => {}", op_name, args.trim(), expected.trim());
        run_op_test(op, args.trim(), expected.trim());
    }
}
