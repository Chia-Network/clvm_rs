#!/usr/bin/env python

import os

def many_args(filename, op, num):
    with open(filename, 'w+') as f:
        f.write('''
;(mod (n)
;    (defun large-atom (n)
;        (if n (lsh (large-atom (- n 1)) 65535) 0x80)
;    )
;    (defun raise (atom)
;        (%s atom ... )
;    )
;    (raise (large-atom n))
;)
''' % op)

        f.write('(a (q 2 6 (c 2 (c (a 4 (c 2 (c 5 (q)))) (q)))) (c (q (a (i 5 (q 23 (a 4 (c 2 (c (- 5 (q . 1)) (q)))) (q . 0x00ffff)) (q 1 . -128)) 1) %s' % op)
        f.write(' 5' * num)
        f.write(') 1))')

    with open(filename[:-4] + 'env', 'w+') as f:
        f.write('(200)')

def many_args_point(filename, op, num):
    with open(filename, 'w+') as f:
        f.write(''';(mod (n)
;    (defun raise (atom)
;        (%s atom atom atom atom atom atom atom atom atom atom atom atom atom atom)
;    )
;    (raise (logxor n 0xb3b8ac537f4fd6bde9b26221d49b54b17a506be147347dae5d081c0a6572b611d8484e338f3432971a9823976c6a232b))
;)
''' % op)
        f.write('(a (q 2 2 (c 2 (c (logxor 5 (q . 0xb3b8ac537f4fd6bde9b26221d49b54b17a506be147347dae5d081c0a6572b611d8484e338f3432971a9823976c6a232b)) (q)))) (c (q %s' % op)
        f.write(' 5' * num)
        f.write(') 1)))')

    with open(filename[:-4] + 'env', 'w+') as f:
        f.write('(0)')

def softfork_wrap(filename, val):

    with open(filename, 'w+') as f:
        f.write(''';(mod (n)
;    (defun recurse (count)
;        (if (= 0 count) 42 (recurse (+ (- count 1) (softfork %s))))
;    )
;    (recurse n)
;)

(a (q 2 2 (c 2 (c 5 (q)))) (c (q 2 (i (= (q) 5) (q 1 . 42) (q 2 2 (c 2 (c (+ (- 5 (q . 1)) (softfork (q . %s))) (q))))) 1) 1))
''' % (val, val))

    with open(filename[:-4] + 'env', 'w+') as f:
        f.write('(0xffffffff)')

def serialized_atom_overflow(filename, size):
    with open(filename, 'w+') as f:
        if size == 0:
            size_blob = b"\x80"
        elif size < 0x40:
            size_blob = bytes([0x80 | size])
        elif size < 0x2000:
            size_blob = bytes([0xc0 | (size >> 8), (size >> 0) & 0xff])
        elif size < 0x100000:
            size_blob = bytes([0xe0 | (size >> 16), (size >> 8) & 0xff, (size >> 0) & 0xFF])
        elif size < 0x8000000:
            size_blob = bytes(
            [
                0xF0 | (size >> 24),
                (size >> 16) & 0xff,
                (size >> 8) & 0xff,
                (size >> 0) & 0xff,
            ]
        )
        elif size < 0x400000000:
            size_blob = bytes(
                [
                    0xF8 | (size >> 32),
                    (size >> 24) & 0xff,
                    (size >> 16) & 0xff,
                    (size >> 8) & 0xff,
                    (size >> 0) & 0xff,
                ]
            )
        else:
            size_blob = bytes(
                [
                    0xfc | ((size >> 40) & 0xff),
                    (size >> 32) & 0xff,
                    (size >> 24) & 0xff,
                    (size >> 16) & 0xff,
                    (size >> 8) & 0xff,
                    (size >> 0) & 0xff,
                ]
            )
        f.write(size_blob.hex())
        f.write("01" * 1000)

try:
    os.mkdir('programs')
except:
    pass

many_args('programs/args-mul.clvm', '*', 300)
many_args('programs/args-add.clvm', '+', 6000)
many_args('programs/args-sub.clvm', '-', 6000)
many_args('programs/args-sha.clvm', 'sha256', 300)
many_args('programs/args-cat.clvm', 'concat', 1200)
many_args('programs/args-any.clvm', 'any', 12000)
many_args('programs/args-all.clvm', 'all', 12000)
many_args('programs/args-and.clvm', 'logand', 6000)
many_args('programs/args-or.clvm', 'logior', 6000)
many_args('programs/args-xor.clvm', 'logxor', 6000)
many_args_point('programs/args-point_add.clvm', 'point_add', 12000)
many_args('programs/args-unknown-1.clvm', '0x7fffffff00', 300)
many_args('programs/args-unknown-2.clvm', '0x7fff40', 300)
many_args('programs/args-unknown-3.clvm', '0x7fff80', 300)
many_args('programs/args-unknown-4.clvm', '0x7fffc0', 300)

# this program attempts to wrap around a 64 bit cost counter
softfork_wrap('programs/softfork-1.clvm', '0x00ffffffffffffff45')
# this program attempts to wrap around a 32 bit cost counter
softfork_wrap('programs/softfork-2.clvm', '0x00ffffff45')

# tests that try to trick a parser into allocating too much memory
serialized_atom_overflow('programs/large-atom-1.hex', 0xffffffff)
serialized_atom_overflow('programs/large-atom-2.hex', 0x3ffffffff)
serialized_atom_overflow('programs/large-atom-3.hex', 0xffffffffff)
serialized_atom_overflow('programs/large-atom-4.hex', 0xfffffffffff)

