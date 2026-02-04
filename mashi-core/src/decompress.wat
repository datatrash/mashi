(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_u32" (func $log_u32 (param i32)))

    (global $src_ptr (mut i32) (i32.const 0))
    (global $dest_ptr (mut i32) (i32.const 1048576))
    (global $code_section_start (mut i32) (i32.const 0))
    (global $code_section_end (mut i32) (i32.const 0))
    (global $output_size (mut i32) (i32.const 0))

    (global $rd_code (mut i32) (i32.const 0))
    (global $rd_range (mut i32) (i32.const -1))

    (global $marker_bit_prob (mut i32) (i32.const 2048))
    (global $byte_index (mut i32) (i32.const 0))

    (global $marker_bit (mut i32) (i32.const 0))

    (memory (export "memory") 8192) ;; reserve 512mb
    ;;(memory (export "memory") 32)

    (func $range_decode_bit (param $prob i32) (result i32)
        (local $bound i32)
        (local $bit i32)

        (local.set $bound
            (i32.mul
                (i32.shr_u (global.get $rd_range) (i32.const 12))
                (local.get $prob)
            )
        )

        (if (i32.lt_u (global.get $rd_code) (local.get $bound))
            (then
                (global.set $rd_range (local.get $bound))
                (local.set $bit (i32.const 1))
            )
            (else
                (global.set $rd_code (i32.sub (global.get $rd_code) (local.get $bound)))
                (global.set $rd_range (i32.sub (global.get $rd_range) (local.get $bound)))
            )
        )

        (block $rd_adjust_loop_end
            (loop $rd_adjust_loop
                (i32.ge_u (global.get $rd_range) (i32.const 0x01000000))
                (br_if $rd_adjust_loop_end)

                (global.get $rd_code)
                (i32.shl (i32.const 8))
                (i32.or (i32.load8_u (global.get $src_ptr)))
                (global.set $rd_code)
                (global.set $src_ptr (i32.add (global.get $src_ptr) (i32.const 1)))
                (global.set $rd_range (i32.shl (global.get $rd_range) (i32.const 8)))

                (br $rd_adjust_loop)
            )
        )

        (local.get $bit)
    )

    (func (export "decompress")
        (local $i i32)

        (global.set $code_section_start (i32.load (global.get $src_ptr)))
        (global.set $code_section_end (i32.load (i32.add (global.get $src_ptr) (i32.const 4))))
        (global.set $output_size (i32.load (i32.add (global.get $src_ptr) (i32.const 8))))
        (global.set $src_ptr (i32.const 12))

        (loop $rd_init_loop
            (global.get $rd_code)
            (i32.shl (i32.const 8))
            (i32.or (i32.load8_u (global.get $src_ptr)))
            (global.set $rd_code)
            (global.set $src_ptr (i32.add (global.get $src_ptr) (i32.const 1)))

            ;; break when we read 4 bytes (and src_ptr is at 16)
            (i32.lt_s (global.get $src_ptr) (i32.const 16))
            (br_if $rd_init_loop)
        )

        (block $decode_loop_end
            (loop $decode_loop
                (i32.ge_u (global.get $byte_index) (global.get $output_size))
                (br_if $decode_loop_end)

                (i32.and (global.get $byte_index) (i32.const 0x1fff)) ;; 0x1fff = BLOCK_MASK
                (if (then) (else
                    (global.set $marker_bit (call $range_decode_bit (global.get $marker_bit_prob)))

                    (global.set $marker_bit_prob
                        (i32.shr_u
                            (i32.add
                                (global.get $marker_bit_prob)
                                (if (result i32) (i32.eq (global.get $marker_bit) (i32.const 0))
                                    (then (i32.const 1))
                                    (else (i32.const 4095))
                                )
                            )
                            (i32.const 1)
                        )
                    )

                    (if (i32.eq (global.get $marker_bit) (i32.const 0)) (then
                        (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 0x2000))) ;; skip block_size
                        br $decode_loop
                    ))
                ))

                (local.set $i (i32.const 0))
                (i32.store8 (global.get $dest_ptr) (i32.const 0))
                (loop $decode_byte_loop
                    (i32.store8 (global.get $dest_ptr)
                        (i32.or
                            (i32.shl (i32.load8_u (global.get $dest_ptr)) (i32.const 1))
                            (call $range_decode_bit (i32.const 2048))
                        )
                    )
                    (br_if $decode_byte_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
                )

                (global.set $dest_ptr (i32.add (global.get $dest_ptr) (i32.const 1)))
                (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 1)))
                br $decode_loop
            )
        )
    )
)