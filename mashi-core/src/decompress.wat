(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_u32" (func $log_u32 (param i32)))

    (global $src_ptr (mut i32) (i32.const 0))
    (global $dest_ptr (mut i32) (i32.const 1048576))
    (global $code_section_start (mut i32) (i32.const 0))
    (global $code_section_end (mut i32) (i32.const 0))
    (global $output_size (mut i32) (i32.const 0))
    (global $byte_index (mut i32) (i32.const 0))

    ;; range decoder state
    (global $rd_code (mut i32) (i32.const 0))
    (global $rd_range (mut i32) (i32.const -1))

    ;; model state
    ;;histories
    ;;dis_model
    ;;dis_model_state
    ;;hash_table
    ;;dis_model_contexts
    ;;context_model_byte_hashes
    ;;context_model_hashes
    ;;context_model_indirect_prob_indices
    ;;stage_1_probs
    ;;stage_1_weight_contexts
    ;;stage_2_probs
    ;;stage_2_prob
    (global $stretch_tab_ptr i32 (i32.const 1032192)) ;; last 4096*4 bytes before decompressed data
    ;;apm_mix_weights
    (global $bit_history (mut i32) (i32.const 0))
    (global $bit_index (mut i32) (i32.const 0))
    (global $bit_history_hash (mut i32) (i32.const 0))
    ;;num_active_context_models
    ;;num_model_outputs
    ;;match_models

    ;; squash_tab
    (data (i32.const 1032060)
        "\01\00\00\00\02\00\00\00\03\00\00\00\06\00\00\00"
        "\0a\00\00\00\10\00\00\00\1b\00\00\00\2d\00\00\00"
        "\49\00\00\00\78\00\00\00\c2\00\00\00\36\01\00\00"
        "\e8\01\00\00\eb\02\00\00\4d\04\00\00\0a\06\00\00"
        "\ff\07\00\00\f5\09\00\00\b2\0b\00\00\14\0d\00\00"
        "\17\0e\00\00\c9\0e\00\00\3d\0f\00\00\87\0f\00\00"
        "\b6\0f\00\00\d2\0f\00\00\e4\0f\00\00\ef\0f\00\00"
        "\f5\0f\00\00\f9\0f\00\00\fc\0f\00\00\fd\0f\00\00"
        "\fe\0f\00\00"
    )

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

    (func $get_squash_tab_value (param $d i32) (result i32)
        (i32.load offset=1032060
            (i32.shl
                (local.get $d)
                (i32.const 2)
            )
        )
    )

    (func $squash (export "squash") (param $d i32) (result i32)
        (local $w i32)

        (if (result i32) (i32.gt_s (local.get $d) (i32.const 2047))
            (then (i32.const 4095))
            (else
                (if (result i32) (i32.lt_s (local.get $d) (i32.const -2047))
                    (then (i32.const 0))
                    (else
                        (local.set $w (i32.and (local.get $d) (i32.const 0x7f)))
                        (local.set $d
                            (i32.add
                                (i32.shr_s (local.get $d) (i32.const 7))
                                (i32.const 16)
                            )
                        )

                        (i32.add
                            (call $get_squash_tab_value (local.get $d))
                            (i32.shr_s
                                (i32.add
                                    (i32.mul
                                        (i32.sub
                                            (call $get_squash_tab_value (i32.add (local.get $d) (i32.const 1)))
                                            (call $get_squash_tab_value (local.get $d))
                                        )
                                        (local.get $w)
                                    )
                                    (i32.const 64)
                                )
                                (i32.const 7)
                            )
                        )
                    )
                )
            )
        )
    )

    (func $model_init
        (local $i i32)
        (local $j i32)
        (local $x i32)
        (local $pi i32)

        ;; init stretch_tab
        (local.set $x (i32.const -2047))
        (loop $stretch_tab_loop
            (local.set $i (call $squash (local.get $x)))
            (local.set $j (local.get $pi))
            (block $st_fill_loop_end
                (loop $st_fill_loop
                    (br_if $st_fill_loop_end (i32.gt_u (local.get $j) (local.get $i)))
                    (i32.store (i32.add (global.get $stretch_tab_ptr) (i32.shl (local.get $j) (i32.const 2))) (local.get $x))
                    (local.set $j (i32.add (local.get $j) (i32.const 1)))
                    (br $st_fill_loop)
                )
            )
            (local.set $pi (i32.add (local.get $i) (i32.const 1)))
            (br_if $stretch_tab_loop (i32.lt_s (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 2049)))
        )
    )

    (func $model_prob (result i32)
        (i32.const 2048)
    )

    (func $model_update (param $bit i32))

    (func (export "decompress")
        (local $i i32)
        (local $bit i32)
        (local $marker_bit i32)
        (local $marker_bit_prob i32)
        (local.set $marker_bit_prob (i32.const 2048))

        (global.set $code_section_start (i32.load (global.get $src_ptr)))
        (global.set $code_section_end (i32.load offset=4 (global.get $src_ptr)))
        (global.set $output_size (i32.load offset=8 (global.get $src_ptr)))
        (global.set $src_ptr (i32.const 12))

        (call $model_init)

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
                    (local.set $marker_bit (call $range_decode_bit (local.get $marker_bit_prob)))

                    (local.set $marker_bit_prob
                        (i32.shr_u
                            (i32.add
                                (local.get $marker_bit_prob)
                                (if (result i32) (i32.eq (local.get $marker_bit) (i32.const 0))
                                    (then (i32.const 1))
                                    (else (i32.const 4095))
                                )
                            )
                            (i32.const 1)
                        )
                    )

                    (if (i32.eq (local.get $marker_bit) (i32.const 0)) (then
                        (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 0x2000))) ;; skip block_size
                        br $decode_loop
                    ))
                ))

                (local.set $i (i32.const 0))
                (loop $decode_byte_loop
                    (local.set $bit (call $range_decode_bit (call $model_prob)))
                    (i32.store8 (global.get $dest_ptr)
                        (i32.or
                            (i32.shl (i32.load8_u (global.get $dest_ptr)) (i32.const 1))
                            (local.get $bit)
                        )
                    )
                    (call $model_update (local.get $bit))
                    (br_if $decode_byte_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
                )

                (global.set $dest_ptr (i32.add (global.get $dest_ptr) (i32.const 1)))
                (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 1)))
                br $decode_loop
            )
        )
    )
)