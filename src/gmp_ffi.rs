#![allow(non_camel_case_types, non_snake_case)]

use core::ptr::NonNull;
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct mpz_t {
    pub alloc: c_int,
    pub size: c_int,
    pub d: NonNull<c_ulonglong>,
}

type c_int = i32;
type c_long = i64;
type c_ulong = u64;
type c_ulonglong = u64;
type mpz_srcptr = *const mpz_t;
type mpz_ptr = *mut mpz_t;
type bitcnt_t = c_ulong;

extern "C" {
    #[link_name = "__gmpz_init"]
    pub fn mpz_init(x: mpz_ptr);
    #[link_name = "__gmpz_import"]
    pub fn mpz_import(
        rop: mpz_ptr,
        count: usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: *const c_void,
    );
    #[link_name = "__gmpz_add_ui"]
    pub fn mpz_add_ui(rop: mpz_ptr, op1: mpz_srcptr, op2: c_ulong);
    #[link_name = "__gmpz_set"]
    pub fn mpz_set(rop: mpz_ptr, op: mpz_srcptr);
    #[link_name = "__gmpz_export"]
    pub fn mpz_export(
        rop: *mut c_void,
        countp: *mut usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: mpz_srcptr,
    ) -> *mut c_void;
    #[link_name = "__gmpz_sizeinbase"]
    pub fn mpz_sizeinbase(arg1: mpz_srcptr, arg2: c_int) -> usize;
    #[link_name = "__gmpz_fdiv_qr"]
    pub fn mpz_fdiv_qr(q: mpz_ptr, r: mpz_ptr, n: mpz_srcptr, d: mpz_srcptr);
    #[link_name = "__gmpz_fdiv_q"]
    pub fn mpz_fdiv_q(q: mpz_ptr, n: mpz_srcptr, d: mpz_srcptr);
    #[link_name = "__gmpz_fdiv_r"]
    pub fn mpz_fdiv_r(r: mpz_ptr, n: mpz_srcptr, d: mpz_srcptr);
    #[link_name = "__gmpz_fdiv_q_2exp"]
    pub fn mpz_fdiv_q_2exp(q: mpz_ptr, n: mpz_srcptr, b: bitcnt_t);
    #[link_name = "__gmpz_init_set_ui"]
    pub fn mpz_init_set_ui(rop: mpz_ptr, op: c_ulong);
    #[link_name = "__gmpz_init_set_si"]
    pub fn mpz_init_set_si(rop: mpz_ptr, op: c_long);
    #[link_name = "__gmpz_clear"]
    pub fn mpz_clear(x: mpz_ptr);
    #[link_name = "__gmpz_add"]
    pub fn mpz_add(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_sub"]
    pub fn mpz_sub(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_mul"]
    pub fn mpz_mul(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_mul_2exp"]
    pub fn mpz_mul_2exp(rop: mpz_ptr, op1: mpz_srcptr, op2: bitcnt_t);
    #[link_name = "__gmpz_get_si"]
    pub fn mpz_get_si(op: mpz_srcptr) -> c_long;
    #[link_name = "__gmpz_and"]
    pub fn mpz_and(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_ior"]
    pub fn mpz_ior(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_xor"]
    pub fn mpz_xor(rop: mpz_ptr, op1: mpz_srcptr, op2: mpz_srcptr);
    #[link_name = "__gmpz_com"]
    pub fn mpz_com(rop: mpz_ptr, op: mpz_srcptr);
    #[link_name = "__gmpz_cmp"]
    pub fn mpz_cmp(op1: mpz_srcptr, op2: mpz_srcptr) -> c_int;
    #[link_name = "__gmpz_cmp_si"]
    pub fn mpz_cmp_si(op1: mpz_srcptr, op2: c_long) -> c_int;
    #[link_name = "__gmpz_cmp_ui"]
    pub fn mpz_cmp_ui(op1: mpz_srcptr, op2: c_ulong) -> c_int;
}

#[cfg(test)]
type c_char = i8;

#[cfg(test)]
extern "C" {
    #[link_name = "__gmpz_init_set_str"]
    pub fn mpz_init_set_str(rop: mpz_ptr, str: *const c_char, base: c_int) -> c_int;
    #[link_name = "__gmpz_get_str"]
    pub fn mpz_get_str(str: *mut c_char, base: c_int, op: mpz_srcptr) -> *mut c_char;
}

#[inline]
pub unsafe extern "C" fn mpz_neg(rop: mpz_ptr, op: mpz_srcptr) {
    if rop as mpz_srcptr != op {
        mpz_set(rop, op);
    }
    (*rop).size = -(*rop).size;
}

#[inline]
pub unsafe extern "C" fn mpz_get_ui(op: mpz_srcptr) -> c_ulong {
    if { (*op).size } != 0 {
        let p = (*op).d.as_ptr();
        (*p) as c_ulong
    } else {
        0
    }
}
