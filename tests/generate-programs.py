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

        f.write('((c (q (c 6 (c 2 (c ((c 4 (c 2 (c 5 (q))))) (q))))) (c (q ((c (i 5 (q 23 ((c 4 (c 2 (c (- 5 (q . 1)) (q))))) (q . 0x00ffff)) (q 1 . -128)) 1)) %s' % op)
        f.write(' 5' * num)
        f.write(') 1)))')

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
        f.write('((c (q (c 2 (c 2 (c (logxor (q . 0xb3b8ac537f4fd6bde9b26221d49b54b17a506be147347dae5d081c0a6572b611d8484e338f3432971a9823976c6a232b)) (q))))) (c (q %s' % op)
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

((c (q (c 2 (c 2 (c 5 (q))))) (c (q (c (i (= (q) 5) (q 1 . 42) (q (c 2 (c 2 (c (+ (- 5 (q . 1)) (softfork (q . %s))) (q)))))) 1)) 1)))
''' % (val, val))

    with open(filename[:-4] + 'env', 'w+') as f:
        f.write('(0xffffffff)')

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

