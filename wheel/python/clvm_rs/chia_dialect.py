from dataclasses import dataclass
from typing import List


@dataclass
class Dialect:
    KEYWORDS: List[str]

    NULL: bytes
    ONE: bytes
    TWO: bytes
    Q_KW: bytes
    A_KW: bytes
    C_KW: bytes


CHIA_DIALECT = Dialect(
    (
        # core opcodes 0x01-x08
        ". q a i c f r l x "
        # opcodes on atoms as strings 0x09-0x0f
        "= >s sha256 substr strlen concat . "
        # opcodes on atoms as ints 0x10-0x17
        "+ - * / divmod > ash lsh "
        # opcodes on atoms as vectors of bools 0x18-0x1c
        "logand logior logxor lognot . "
        # opcodes for bls 1381 0x1d-0x1f
        "point_add pubkey_for_exp . "
        # bool opcodes 0x20-0x23
        "not any all . "
        # misc 0x24
        "softfork "
    ).split(),
    NULL=bytes.fromhex(""),
    ONE=bytes.fromhex("01"),
    TWO=bytes.fromhex("02"),
    Q_KW=bytes.fromhex("01"),
    A_KW=bytes.fromhex("02"),
    C_KW=bytes.fromhex("04"),
)
