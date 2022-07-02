(mod input

  ; test with:
  ; ```
  ; SERIALIZED_HEX=9fffffffffffffffff86666f6f626172fe02fe02fe02fe02fe02fe02fe02fe02
  ; run -d deserialize_w_backrefs.cl > deserialize_w_backrefs.hex
  ; brun -x deserialize_w_backrefs.hex $SERIALIZED_HEX
  ; ```
  ; Use whatever serialized clvm expression you like for `SERIALIZED_HEX`.
  ; Generate it with `print(sexp.as_bin(allow_backrefs=True).hex())` in python.
  ; It works for standard serialization (without backrefs) too.

  (defconstant MAX_SINGLE_BYTE 0x7F)
  (defconstant MAX_ONE_BYTE 0xbf)
  (defconstant MAX_TWO_BYTE 0xdf)
  (defconstant MAX_THREE_BYTE 0xef)
  (defconstant MAX_FOUR_BYTE 0xf7)
  (defconstant MAX_FIVE_BYTE 0xfb)
  (defconstant BACK_REFERENCE 0xfe)
  (defconstant CONS_BOX_MARKER 0xFF)

  (defun-inline extract_from_tree (tree_path_pair)
    ; given `tree_path_pair = `(path . tree)` return the `path`th element of `tree` using standard clvm paths
    (a (q a 2 3) tree_path_pair)
  )

  (defun-inline extract_reference (sexp_stack_input_stream_pair)
    ; return sexp_stack_input_stream_pair = (sexp_stack . input_stream)
    (c (c (extract_from_tree (f sexp_stack_input_stream_pair)) (r (f sexp_stack_input_stream_pair))) (r sexp_stack_input_stream_pair))
  )

  (defun sexp_from_stream (sexp_stack_input_stream_pair)
    ; return sexp_stack_input_stream_pair = (sexp_stack . input_stream)
    (if (= (substr (r sexp_stack_input_stream_pair) 0 1) CONS_BOX_MARKER)
      (pop_and_cons (sexp_from_stream (sexp_from_stream (c (f sexp_stack_input_stream_pair) (substr (r sexp_stack_input_stream_pair) 1)))))
      (if (= (substr (r sexp_stack_input_stream_pair) 0 1) BACK_REFERENCE)
        (extract_reference (atom_from_stream (substr (r sexp_stack_input_stream_pair) 1 2)
                                             (c (f sexp_stack_input_stream_pair)
                                                (substr (r sexp_stack_input_stream_pair) 2))
                           )
        )
        (atom_from_stream (substr (r sexp_stack_input_stream_pair) 0 1)
                          (c (f sexp_stack_input_stream_pair)
                             (substr (r sexp_stack_input_stream_pair) 1)))
      )
    )
  )

  (defun pop_and_cons ((sexp_stack . input_stream))
    (c (c (c (f (r sexp_stack)) (f sexp_stack)) (r (r sexp_stack))) input_stream)
  )

  (defun parse_right (left old_cache)
    (cons_sexp_from_stream ())
  )

  (defun cons_sexp_from_stream (left_sexp_with_input cache)
    (cons_return (f left_sexp_with_input) (sexp_from_stream (f (r left_sexp_with_input))) cache)
  )

  (defun cons_return (left_sexp right_sexp_with_input cache)
    (list (c left_sexp (f right_sexp_with_input)) (f (r right_sexp_with_input)))
  )

  (defun atom_from_stream (input_bits sexp_stack_input_stream_pair)
    (if (= input_bits (quote 0x80))
      (c (c 0 (f sexp_stack_input_stream_pair)) (r sexp_stack_input_stream_pair))
      (if (>s input_bits MAX_SINGLE_BYTE)
        (atom_from_stream_part_two (get_bitcount input_bits (r sexp_stack_input_stream_pair)) (f sexp_stack_input_stream_pair))
        (c (c input_bits (f sexp_stack_input_stream_pair)) (r sexp_stack_input_stream_pair))
      )
    )
  )

  ; Note that we reject any serialized atom here with more than 3 bytes of
  ; encoded length prefix, even though the Rust and Python CLVM interpreters
  ; and deserializers support more.
  ; This allows 4 + 8 + 8 = 20 bits = 1MB atoms
  ; Also note that this does not limit intermediate atom length. Those limits
  ; are implemented in the clvm interpreters theselves
  (defun get_bitcount (input_bits input_file)
    ; return `(string_length . input_file_new_start_point)`
    (if (>s input_bits MAX_ONE_BYTE)
      (if (>s input_bits MAX_TWO_BYTE)
        (if (>s input_bits MAX_THREE_BYTE)
          (if (>s input_bits MAX_FOUR_BYTE)
                  (x)
            ;four byte length prefix
            (c (concat 0x00 (logand (quote 0x7) input_bits) (substr input_file 0 3)) (substr input_file 3))
          )
          ;three byte length prefix
          (c (concat 0x00 (logand (quote 0xf) input_bits) (substr input_file 0 2)) (substr input_file 2))
        )
        ;two byte length prefix
        (c (concat 0x00 (logand (quote 0x1f) input_bits) (substr input_file 0 1)) (substr input_file 1))
      )
      ;one byte length prefix
      (c (logand (quote 0x3f) input_bits) input_file)
    )
  )

  (defun atom_from_stream_part_two ((size_to_read . input_file) sexp_stack)
    (c (c (substr input_file 0 size_to_read) sexp_stack) (substr input_file size_to_read))
  )

  ; main
  (f (f (sexp_from_stream (c 0 input))))

)
