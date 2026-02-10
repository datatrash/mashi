(module
    (import "host" "l_i32" (func $l_i32 (param i32)))
    (import "host" "l_dm" (func $l_dm (param i32) (param i32) (param i32) (param i32) (param i32)))
    (import "host" "l_u32" (func $l_u32 (param i32)))
    (import "host" "l_x32" (func $l_x32 (param i32)))
    (import "host" "l_sep" (func $l_sep))

    (global $src_ptr (mut i32) (i32.const 0))
    (global $code_section_start (mut i32) (i32.const 0))
    (global $code_section_end (mut i32) (i32.const 0))
    (global $output_size (mut i32) (i32.const 0))
    (global $byte_index (mut i32) (i32.const 0))

    ;; range decoder state
    (global $rd_code (mut i32) (i32.const 0))
    (global $rd_range (mut i32) (i32.const -1))

    ;; model state
    (global $dis_model_state (mut i32) (i32.const 0))
    (global $stage_2_prob (mut i32) (i32.const 0))
    (global $bit_history (mut i32) (i32.const 0))
    (global $bit_index (mut i32) (i32.const 0))
    (global $bit_history_hash (mut i32) (i32.const 0))
    (global $num_active_context_models (mut i32) (i32.const 21))
    (global $num_model_outputs (mut i32) (i32.const 72))

    ;; offsets:
    ;; stretch_tab = 0x00d0000

    ;; bit_masks = 0x00c0000
    (data (i32.const 0x00c0000)
        "\7f\ff\ff\00\83\e1\bf\ff\7f\ff\cf\00\7f\ff\df\7f"
        "\3f\7b\ff\6f\bf"
    )
    ;; byte_masks = 0x00c1000
    (data (i32.const 0x00c1000)
        "\00\01\03\11\80\02\04\05\35\4c\21\07\0f\06\88\c1"
        "\e0\f3\08\09\1a"
    )

    ;; squash_tab = 0x00f0000
    (data (i32.const 0x00f0000)
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
    ;; byte_history_pos = 0x00f1000
    ;; depack to 0x1000000..0x3000000, so maybe shift everything when we need less memory, meh
    ;; for every different dis_model_state (max 32 of them for now, increase this if we have more than DisModelStates + 1):
    ;;;; apm_indices = 0x0f02000
    ;;;; apm_weights = 0x0f03000
    ;;;; apm_tabs = 0xe000000..0x1ac00000 (32 dismodels, 3 tabs, length per tab = 0x110000 * u16 = 0x220000)
    ;;;; stage_1_weights = 0x25400000..0x26900000 (32 dismodels, 35328 weights per model * i16x8 (= 16) = 0x8a000 per dismodel = 1140000
    ;;;; stage_2_weights = 0x0f0d000..0x0f0d200 (32 dismodels, 16 bytes per weight)
    ;;;; byte_history = 0x3000000..0x5000000 (length per history = 0x100000, bytes are interleaved so byte 0 = dismodelcontext 0, 1 = dismodelcontext 1, 32 = second byte for dismodelcontext 0, etc)
    ;;;; context_indirect_probs = 0x1ac00000..0x25400000 (length per vec = 0x2a0000 * u16 = 0x540000, 32 different indirect_probs tables)
    ;; apm_mix_weights
    (data (i32.const 0x0f04000) "\02\02\01")
    ;; apm_adjust_rates
    (data (i32.const 0x0f04010) "\03\03\02")
    ;; for every match_model (NUM_MATCH_MODELS = 8)
    ;;;; match_model_index_buffer  = 0x5000000..0x5800000 (length per buffer = 0x040000 * 4 bytes = 0x100000)
    ;; match_model_bit_position  = 0x0f05000
    ;; match_model_offset        = 0x0f05100
    ;; match_model_length        = 0x0f05200
    ;; match_model_history_hash  = 0x0f05300
    ;; match_model_predicted_bit = 0x0f05400
    ;; stage_1_probs             = 0x0f06000
    ;; stage_1_weight_contexts   = 0x0f07000
    ;; stage_2_probs             = 0x0f08000
    ;; context_model_byte_hashes = 0x0f09000
    ;; context_model_hashes      = 0x0f0a000
    ;; context_model_indirect_prob_indices = 0x0f0b000
    ;; scratch                   = 0x0f0c000
    ;;
    ;; hash_table                = 0x6000000..0xe000000 (16 bytes per entry)
    ;;;; offset 0x0 = checksums
    ;;;; offset 0x4 = stationary_counts
    ;;;; offset 0x8 = indirect_counts
    ;;;; offset 0xc = run_count
    ;;;; offset 0xe = run_symbol
    ;;
    ;; dis_model_queue           = 0x0f20000..0x0f24000
    (global $dm_depth (mut i32) (i32.const 0))
    (global $dm_read_pos (mut i32) (i32.const 0))
    (global $dm_write_pos (mut i32) (i32.const 3)) ;; change this if the initial bytes loaded in the below table before an opcode change
    (data (i32.const 0x0f20000)
        "\02\03\04"
    )
    (global $dm_temp_leb_val (mut i64) (i64.const 0))
    (global $dm_leb_val (mut i64) (i64.const 0))
    (global $dm_leb_shift (mut i32) (i32.const 0))
    (global $dm_float_bytes_left (mut i32) (i32.const 0))
    (global $dm_opcode (mut i32) (i32.const 0))

    (memory (export "memory") 9808) ;; bomb that tree line about 613mb back

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
        (i32.load offset=0x00f0000
            (i32.shl
                (local.get $d)
                (i32.const 2)
            )
        )
    )

    (func $squash (param $d i32) (result i32)
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

    (func $mul_hi (param $a v128) (param $b v128) (result v128)
        (i16x8.narrow_i32x4_s
            (i32x4.shr_s
                (i32x4.extmul_low_i16x8_s (local.get $a) (local.get $b))
                (i32.const 16)
            )
            (i32x4.shr_s
                (i32x4.extmul_high_i16x8_s (local.get $a) (local.get $b))
                (i32.const 16)
            )
        )
    )

    (func $mix (export "mix") (param $probs_ptr i32) (param $weights_ptr i32) (param $count i32) (result i32)
        (local $x i32)
        (local $acc v128)
        (local $tmp i32)

        (local.set $acc (i16x8.splat (i32.const 0)))

        (loop $acc_loop
            (local.set $acc
                (local.set $tmp (i32.const 0))
                (i16x8.add
                    (local.get $acc)
                    (call $mul_hi (v128.load (local.get $probs_ptr)) (v128.load (local.get $weights_ptr)))
                )
            )

            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 16)))
            (local.set $weights_ptr (i32.add (local.get $weights_ptr) (i32.const 16)))
            (br_if $acc_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (local.get $count)))
        )

        (local.set $x (i32.const 0))
        (loop $horsum_loop
            (local.set $acc
                (i16x8.add
                    (i8x16.shuffle 0 1 4 5 8 9 12 13 16 17 20 21 24 25 28 29 (local.get $acc) (local.get $acc))
                    (i8x16.shuffle 2 3 6 7 10 11 14 15 18 19 22 23 26 27 30 31 (local.get $acc) (local.get $acc))
                )
            )
            (br_if $horsum_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 3)))
        )

        (i16x8.extract_lane_s 0 (local.get $acc))
    )

    (func $train (export "train") (param $probs_ptr i32) (param $weights_ptr i32) (param $count i32) (param $bit i32) (param $current_prob i32)
        (local $prediction_error v128)
        (local $x i32)

        (local.set $prediction_error
            (i16x8.splat
                (i32.mul
                    (i32.sub
                        (i32.shl (local.get $bit) (i32.const 12))
                        (local.get $current_prob)
                    )
                    (i32.const 7)
                )
            )
        )

        (loop $train_loop
            (v128.store
                (local.get $weights_ptr)
                (i16x8.add_sat_s
                    (v128.load (local.get $weights_ptr))
                    (i16x8.shr_s
                        (i16x8.add
                            (call $mul_hi
                                (i16x8.shl
                                    (v128.load (local.get $probs_ptr))
                                    (i32.const 1)
                                )
                                (local.get $prediction_error)
                            )
                            (i16x8.splat (i32.const 1))
                        )
                        (i32.const 1)
                    )
                )
            )

            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 16)))
            (local.set $weights_ptr (i32.add (local.get $weights_ptr) (i32.const 16)))
            (br_if $train_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (local.get $count)))
        )
    )

    (func $model_init (export "model_init")
        (local $i i32)
        (local $j i32)
        (local $x i32)
        (local $pi i32)
        (local $apm_tab_index i32)
        (local $apm_tab_value i32)
        (local $last_bits i32)
        (local $counts_one i32)
        (local $counts_zero i32)

        ;; init stretch_tab
        (local.set $x (i32.const -2047))
        (loop $stretch_tab_loop
            (local.set $i (call $squash (local.get $x)))
            (local.set $j (local.get $pi))
            (block $st_fill_loop_end
                (loop $st_fill_loop
                    (br_if $st_fill_loop_end (i32.gt_u (local.get $j) (local.get $i)))
                    (i32.store offset=0x00d0000 (i32.shl (local.get $j) (i32.const 2)) (local.get $x))
                    (local.set $j (i32.add (local.get $j) (i32.const 1)))
                    (br $st_fill_loop)
                )
            )
            (local.set $pi (i32.add (local.get $i) (i32.const 1)))
            (br_if $stretch_tab_loop (i32.lt_s (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 2049)))
        )

        ;; init apm_tab
        (local.set $i (i32.const 0))
        (local.set $j (i32.const -1))
        (loop $apm_tab_outer_loop
            (local.set $x (i32.const 0))
            (loop $apm_tab_inner_loop
                (local.set $apm_tab_index
                    (i32.shl
                        (local.tee $j (i32.add (local.get $j) (i32.const 1)))
                        (i32.const 1)
                    )
                )
                (local.set $apm_tab_value
                    (call $squash
                        (i32.sub
                            (i32.shl (local.get $x) (i32.const 8))
                            (i32.const 2047)
                        )
                    )
                )
                (i32.store16 offset=0xe000000 (local.get $apm_tab_index) (local.get $apm_tab_value))
                (br_if $apm_tab_inner_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 17)))
            )
            (br_if $apm_tab_outer_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 0x10000)))
        )

        ;; copy apm tab to other dismodels
        (local.set $x (i32.const 1))
        (loop $copy_apm_tabs_loop
            (memory.copy
                (i32.add (i32.const 0xe000000) (i32.mul (i32.const 0x220000) (local.get $x)))
                (i32.const 0xe000000)
                (i32.const 0x220000)
            )
            (br_if $copy_apm_tabs_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 96)))
        )

        ;; init indirect_probs
        (local.set $i (i32.const 0))
        (local.set $counts_one (i32.const 0))
        (loop $ip_counts_one_loop
            (local.set $counts_zero (i32.const 0))
            (loop $ip_counts_zero_loop
                (i32.store16 offset=0x1ac00000 (local.get $i)
                    (i32.div_s
                        (i32.shl
                            (i32.add
                                (i32.shl (local.get $counts_one) (i32.const 4))
                                (i32.const 1)
                            )
                            (i32.const 16)
                        )
                        (i32.add
                            (i32.add
                                (i32.shl (local.get $counts_zero) (i32.const 4))
                                (i32.shl (local.get $counts_one) (i32.const 4))
                            )
                            (i32.const 2)
                        )
                    )
                )

                (local.set $i (i32.add (local.get $i) (i32.const 2)))
                (br_if $ip_counts_zero_loop (i32.lt_u (local.tee $counts_zero (i32.add (local.get $counts_zero) (i32.const 1))) (i32.const 64)))
            )
            (br_if $ip_counts_one_loop (i32.lt_u (local.tee $counts_one (i32.add (local.get $counts_one) (i32.const 1))) (i32.const 64)))
        )

        ;; copy indirect_probs
        (local.set $x (i32.const 1))
        (loop $copy_indirect_probs_loop
            (memory.copy
                (i32.add (i32.const 0x1ac00000) (i32.mul (i32.const 0x2000) (local.get $x)))
                (i32.const 0x1ac00000)
                (i32.const 0x2000)
            )

            ;; 21504 = NUM_MAX_ACTIVE_CONTEXT_MODELS (42) * 16 * 32 possible dismodelstates, see DisModelContext::new() for the first two
            (br_if $copy_indirect_probs_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 21504)))
        )
    )

    (func $hash_byte (param $state i32) (param $value i32) (result i32)
        (i32.xor
            (i32.mul
                (local.get $state)
                (i32.const 16777619)
            )
            (local.get $value)
        )
    )

    (func $get_apm_data_offset (param $apm_index i32) (result i32)
        (i32.shl
            (i32.add
                (i32.mul (global.get $dis_model_state) (i32.const 3)) ;; 3 apm stages per dis_model_context
                (local.get $apm_index)
            )
            (i32.const 2) ;; 4 bytes per element
        )
    )

    (func $get_apm_table_offset (param $apm_index i32) (result i32)
        (i32.add (i32.const 0xe000000 (;apm_tab;)) (i32.mul (i32.const 0x220000)
            (i32.add
                (i32.mul (global.get $dis_model_state) (i32.const 3)) ;; 3 apm stages per dis_model_context
                (local.get $apm_index)
            )
        ))
    )

    (func $apm_stage_update (export "apm_stage_update") (param $apm_index i32) (param $bit i32)
        (local $index i32)
        (local $table_offset i32)
        (local.set $index (i32.load offset=0x0f02000 (call $get_apm_data_offset (local.get $apm_index)))) ;; apm_indices
        (call $apm_stage_update_entry (call $get_apm_table_offset (local.get $apm_index)) (i32.load8_s offset=0x0f04010 (local.get $apm_index)) (local.get $index) (local.get $bit)) ;; apm_adjust_rates
        (call $apm_stage_update_entry (call $get_apm_table_offset (local.get $apm_index)) (i32.load8_s offset=0x0f04010 (local.get $apm_index)) (i32.add (local.get $index) (i32.const 1)) (local.get $bit))
    )

    (func $apm_stage_update_entry (param $table_offset i32) (param $adjust_rate i32) (param $index i32) (param $bit i32)
        (local $entry i32)
        (local $mempos i32)
        (local.set $mempos (i32.add (local.get $table_offset) (i32.shl (local.get $index) (i32.const 1))))
        (local.set $entry (i32.load16_s (local.get $mempos)))
        (local.set $entry
            (i32.add
                (local.get $entry)
                (i32.shr_s
                    (i32.sub
                        (i32.shl (local.get $bit) (i32.const 12))
                        (local.get $entry)
                    )
                    (local.get $adjust_rate)
                )
            )
        )
        (i32.store16 (local.get $mempos) (local.get $entry))
    )

    (func $apm_stage_prob (export "apm_stage_prob") (param $apm_index i32) (result i32)
        (local $index i32)
        (local $weight i32)
        (local $a i32)
        (local $b i32)
        (local.set $index (i32.load offset=0x0f02000 (call $get_apm_data_offset (local.get $apm_index)))) ;; apm_indices
        (local.set $weight (i32.load offset=0x0f03000 (call $get_apm_data_offset (local.get $apm_index)))) ;; apm_weights
        (local.set $a (i32.load16_s (i32.add (call $get_apm_table_offset (local.get $apm_index)) (i32.shl (local.get $index) (i32.const 1)))))
        (local.set $b (i32.load16_s (i32.add (call $get_apm_table_offset (local.get $apm_index)) (i32.shl (i32.add (local.get $index) (i32.const 1)) (i32.const 1)))))

        (i32.add
            (local.get $a)
            (i32.shr_s
                (i32.mul
                    (i32.sub (local.get $b) (local.get $a))
                    (local.get $weight)
                )
                (i32.const 8)
            )
        )
    )

    (func $apm_stage_set_index (export "apm_stage_set_index") (param $apm_index i32) (param $context i32) (param $prob i32)
        (local.set $context (i32.and (local.get $context) (i32.const 0xffff)))
        (local.set $prob (i32.add (local.get $prob) (i32.const 2047)))
        (if (i32.lt_s (local.get $prob) (i32.const 0)) (then (local.set $prob (i32.const 0))))
        (if (i32.gt_s (local.get $prob) (i32.const 4095)) (then (local.set $prob (i32.const 4095))))

        (i32.store offset=0x0f02000 (call $get_apm_data_offset (local.get $apm_index))
            (i32.add
                (i32.mul (local.get $context) (i32.const 17))
                (i32.shr_u (local.get $prob) (i32.const 8))
            )
        ) ;; apm_indices
        (i32.store offset=0x0f03000 (call $get_apm_data_offset (local.get $apm_index)) (i32.and (local.get $prob) (i32.const 0xff))) ;; apm_weights
    )

    (func $history_get (export "history_get") (param $history_idx i32) (param $index i32) (result i32)
        ;; byte_history[0x3000000 + ((index & HISTORY_BUFFER_LEN - 1) * 32) + history_idx]
        (i32.load8_u offset=0x3000000
            (i32.add
                (i32.shl
                    (i32.and
                        (local.get $index)
                        (i32.const 0x00fffff)
                    )
                    (i32.const 5)
                )
                (local.get $history_idx)
            )
        )
    )

    (func $history_get_byte_history_pos (param $history_idx i32) (result i32)
        (i32.load offset=0x00f1000 (;byte_history_pos;) (i32.shl (local.get $history_idx) (i32.const 2)))
    )

    (func $history_update (export "history_update") (param $history_idx i32) (param $byte i32)
        (local $byte_history_pos i32)
        (local.set $byte_history_pos (call $history_get_byte_history_pos (local.get $history_idx)))

        ;; byte_history[0x3000000 + (byte_history_pos * 32) + history_idx] = byte
        (i32.store8 offset=0x3000000
            (i32.add
                (i32.shl
                    (local.get $byte_history_pos)
                    (i32.const 5)
                )
                (local.get $history_idx)
            )
            (local.get $byte)
        )

        (i32.store offset=0x00f1000 (;byte_history_pos;)
            (i32.shl (local.get $history_idx) (i32.const 2))
            (i32.and
                (i32.add (local.get $byte_history_pos) (i32.const 1))
                (i32.const 0xfffff)
            )
        )
    )

    (func $history_hash (export "history_hash") (param $history_idx i32) (param $byte_mask i32) (result i32)
        (local $i i32)
        (local $state i32)
        (local.set $state (i32.const 2166136261))
        (local.set $state (call $hash_byte (local.get $state) (local.get $byte_mask)))

        (loop $bit_loop
            (if (i32.and (i32.shr_u (local.get $byte_mask) (local.get $i)) (i32.const 1))
                (then
                    (local.set $state (call $hash_byte (local.get $state)
                        (call $history_get (local.get $history_idx)
                            (i32.sub
                                (i32.sub
                                    (i32.load offset=0x00f1000 (i32.shl (local.get $history_idx) (i32.const 2))) ;; byte_history_pos
                                    (i32.const 1)
                                )
                                (local.get $i)
                            )
                        )
                    ))
                )
            )
            (br_if $bit_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )

        (local.get $state)
    )

    (func $match_model_prob (export "match_model_prob") (param $match_model_index i32) (result i32)
        (local $length i32)
        (local $predicted_bit i32)

        (local.set $length (i32.load offset=0x0f05200 (;match_model_length;) (i32.shl (local.get $match_model_index) (i32.const 2))))
        (if (result i32) (local.get $length)
            (then
                (local.set $predicted_bit
                    (i32.and
                        (i32.shr_u
                            (call $history_get
                                (i32.const 0)
                                (i32.sub
                                    (call $history_get_byte_history_pos (i32.const 0))
                                    (i32.load offset=0x0f05100 (;match_model_offset;) (i32.shl (local.get $match_model_index) (i32.const 2)))
                                )
                            )
                            (i32.sub
                                (i32.const 7)
                                (i32.load offset=0x0f05000 (;match_model_bit_position;) (i32.shl (local.get $match_model_index) (i32.const 2)))
                            )
                        )
                        (i32.const 0x01)
                    )
                )
                (i32.store offset=0x0f05400 (;match_model_predicted_bit;) (i32.shl (local.get $match_model_index) (i32.const 2)) (local.get $predicted_bit))
                (i32.and
                    (i32.mul
                        (i32.div_s
                            (i32.const 2048)
                            (local.get $length)
                        )
                        (i32.add (i32.mul (local.get $predicted_bit) (i32.const -2)) (i32.const 1))
                    )
                    (i32.const 0x0fff)
                )
            )
            (else (i32.const 2048))
        )
    )

    (func $match_model_update_bit (export "match_model_update_bit") (param $match_model_index i32) (param $bit i32)
        (if
            (i32.ne
                (i32.load offset=0x0f05400 (;match_model_predicted_bit;) (i32.shl (local.get $match_model_index) (i32.const 2)))
                (local.get $bit)
            )
            (then
                (i32.store offset=0x0f05200 (;match_model_length;) (i32.shl (local.get $match_model_index) (i32.const 2)) (i32.const 0))
            )
        )
        (i32.store offset=0x0f05000 (;match_model_bit_position;) (i32.shl (local.get $match_model_index) (i32.const 2))
            (i32.add
                (i32.load offset=0x0f05000 (;match_model_bit_position;) (i32.shl (local.get $match_model_index) (i32.const 2)))
                (i32.const 1)
            )
        )
    )

    (func $match_model_update_byte (export "match_model_update_byte") (param $match_model_index i32) (param $byte_mask i32)
        (local $history_pos i32)
        (local $length i32)
        (local $offset i32)
        (local $index_buffer_offset i32)

        (i32.store offset=0x0f05000 (;match_model_bit_position;) (i32.shl (local.get $match_model_index) (i32.const 2)) (i32.const 0))
        (local.set $history_pos (i32.sub (call $history_get_byte_history_pos (i32.const 0)) (i32.const 1)))

        (local.set $length (i32.load offset=0x0f05200 (;match_model_length;) (i32.shl (local.get $match_model_index) (i32.const 2))))
        (local.set $offset (i32.load offset=0x0f05100 (;match_model_offset;) (i32.shl (local.get $match_model_index) (i32.const 2))))
        (local.set $index_buffer_offset
            (i32.add
                (i32.mul (local.get $match_model_index) (i32.const 0x100000))
                (i32.shl
                    (i32.load offset=0x0f05300 (;match_model_history_hash;) (i32.shl (local.get $match_model_index) (i32.const 2)))
                    (i32.const 2)
                )
            )
        )

        (if (local.get $length)
            (then
                ;; > 0 && < 255 ?
                (if (i32.lt_u (local.get $length) (i32.const 255)) (then
                    (local.set $length (i32.add (local.get $length) (i32.const 1)))
                ))
            ) (else
                ;; == 0
                (local.set $offset
                    (i32.sub
                        (local.get $history_pos)
                        (i32.load offset=0x5000000 (;match_model_index_buffer;) (local.get $index_buffer_offset))
                    )
                )
                (if (i32.and (local.get $offset) (i32.const 0xfffff)) (then
                    (block $update_byte_loop_end
                        (loop $update_byte_loop
                            (br_if $update_byte_loop_end (i32.eq (local.get $length) (i32.const 255)))
                            (br_if $update_byte_loop_end
                                (i32.ne
                                    (call $history_get (i32.const 0) (i32.sub (local.get $history_pos) (local.get $length)))
                                    (call $history_get (i32.const 0) (i32.sub (i32.sub (local.get $history_pos) (local.get $length)) (local.get $offset)))
                                )
                            )

                            (local.set $length (i32.add (local.get $length) (i32.const 1)))
                            br $update_byte_loop
                        )
                    )
                ))
            )
        )

        (i32.store offset=0x0f05200 (;match_model_length;) (i32.shl (local.get $match_model_index) (i32.const 2)) (local.get $length))
        (i32.store offset=0x0f05100 (;match_model_offset;) (i32.shl (local.get $match_model_index) (i32.const 2)) (local.get $offset))
        (i32.store offset=0x5000000 (;match_model_index_buffer;) (local.get $index_buffer_offset) (local.get $history_pos))

        (i32.store offset=0x0f05300 (;match_model_history_hash;) (i32.shl (local.get $match_model_index) (i32.const 2))
            (i32.and
                (call $history_hash (i32.const 0) (local.get $byte_mask))
                (i32.const 0x3ffff)
            )
        )
    )

    (func $stretch (param $input i32) (result i32)
        (i32.load offset=0x00d0000 (;stretch_tab;) (i32.shl (local.get $input) (i32.const 2)))
    )

    (func $get_stage_1_weights_ptr (param $i i32) (result i32)
        (i32.add
            (i32.const 0x25400000 (;stage_1_weights;))
            (i32.add
                (i32.mul
                    (global.get $dis_model_state)
                    (i32.const 0x8a000)
                )
                (i32.shl
                    (i32.mul
                        (i32.add
                            (i32.shl (local.get $i) (i32.const 8))
                            (i32.load offset=0x0f07000 (;stage_1_weight_contexts;) (i32.shl (local.get $i) (i32.const 2)))
                        )
                        (i32.const 17) ;; NUM_MAX_MODEL_OUTPUTS / MIX_VECTOR_SIZE
                    )
                    (i32.const 4)
                )
            )
        )
    )

    (func $model_prob (export "model_prob") (result i32)
        (local $i i32)
        (local $tmp i32)
        (local $prob i32)
        (local $apm_context i32)
        (local $probs_ptr i32)

        (local $hash i32)
        (local $checksum i32)
        (local $bucket_index i32)

        (global.set $bit_history_hash
            (i32.or
                (i32.shl (i32.const 1) (global.get $bit_index))
                (global.get $bit_history)
            )
        )

        (local.set $i (i32.const 0))
        (loop $update_context_models_loop
            (local.set $checksum
                (local.tee $hash
                    (i32.xor
                        (i32.load offset=0x0f09000 (;context_model_byte_hashes;) (i32.shl (local.get $i) (i32.const 2)))
                        (i32.or
                            (i32.shl (i32.const 1) (global.get $bit_index))
                            (i32.and
                                (global.get $bit_history)
                                (i32.load8_u offset=0x00c0000 (;bit_masks;) (i32.rem_u (local.get $i) (i32.const 21)))
                            )
                        )
                    )
                )
            )

            (local.set $hash
                (i32.shl
                    (i32.and
                        (local.get $hash)
                        (i32.const 2097151)
                    )
                    (i32.const 2)
                )
            )

            (local.set $bucket_index (i32.const 0))

            (block $check_checksums_in_bucket
                (loop $check_checksums_in_bucket_loop
                    (br_if $check_checksums_in_bucket (i32.eq (local.get $bucket_index) (i32.const 4)))
                    (br_if $check_checksums_in_bucket (i32.eq (local.get $checksum) (i32.load offset=0x6000000 (;ht_checksums;) (i32.shl (local.get $hash) (i32.const 4)))))

                    (local.set $hash (i32.add (local.get $hash) (i32.const 1)))
                    (local.set $bucket_index (i32.add (local.get $bucket_index) (i32.const 1)))

                    (br $check_checksums_in_bucket_loop)
                )
            )

            (if (i32.eq (local.get $bucket_index) (i32.const 4))
                (then
                    (local.set $hash (i32.sub (local.get $hash) (i32.const 1)))
                    (local.set $bucket_index (i32.sub (local.get $bucket_index) (i32.const 1)))

                    ;; new hash table entry, only requires stationary_count to be set, the rest can be zeroed
                    (memory.fill (i32.add (i32.const 0x6000000) (i32.shl (local.get $hash) (i32.const 4))) (i32.const 0) (i32.const 16))
                    (i32.store offset=0x6000004 (;ht_stationary_counts;) (i32.shl (local.get $hash) (i32.const 4)) (i32.const 2097152))
                )
            )

            (block $swap_ht_entries
                (loop $swap_ht_entries_loop
                    (br_if $swap_ht_entries (i32.eq (local.get $bucket_index) (i32.const 0)))

                    ;; copy hash table entry 'hash' to scratch area
                    (memory.copy
                        (i32.const 0x0f0c000 (;scratch;))
                        (i32.add (i32.const 0x6000000) (i32.shl (local.get $hash) (i32.const 4)))
                    (i32.const 16))

                    ;; copy 'hash - 1' to 'hash'
                    (memory.copy
                        (i32.add (i32.const 0x6000000) (i32.shl (local.get $hash) (i32.const 4)))
                        (i32.add (i32.const 0x5fffff0) (i32.shl (local.get $hash) (i32.const 4)))
                    (i32.const 16))

                    (local.set $hash (i32.sub (local.get $hash) (i32.const 1)))
                    (local.set $bucket_index (i32.sub (local.get $bucket_index) (i32.const 1)))

                    ;; copy scratch back
                    (memory.copy
                        (i32.add (i32.const 0x6000000) (i32.shl (local.get $hash) (i32.const 4)))
                        (i32.const 0x0f0c000 (;scratch;))
                    (i32.const 16))

                    (br $swap_ht_entries_loop)
                )
            )

            (i32.store offset=0x6000000 (;ht_checksums;) (i32.shl (local.get $hash) (i32.const 4)) (local.get $checksum))
            (i32.store offset=0x0f0a000 (;context_model_hashes;) (i32.shl (local.get $i) (i32.const 2)) (local.get $hash))

            ;; indirect model
            (local.set $tmp
                (i32.or
                    (i32.shl (local.get $i) (i32.const 16))
                    (i32.load offset=0x6000008 (;ht_indirect_counts;) (i32.shl (local.get $hash) (i32.const 4)))
                )
            )
            (i32.store offset=0x0f0b000 (;context_model_indirect_prob_indices;) (i32.shl (local.get $i) (i32.const 2)) (local.get $tmp))

            (i32.store16 offset=0x0f06000 (;stage_1_probs;)
                (local.get $probs_ptr)
                (call $stretch
                    (i32.shr_u
                        (i32.load16_u
                            offset=0x1ac00000 (;context_indirect_probs;)
                            (i32.add
                                (i32.mul (global.get $dis_model_state) (i32.const 0x540000))
                                (i32.shl (local.get $tmp) (i32.const 1))
                            )
                        )
                        (i32.const 4)
                    )
                )
            )
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))

            ;; stationary model
            (i32.store16 offset=0x0f06000 (;stage_1_probs;)
                (local.get $probs_ptr)
                (call $stretch
                    (i32.shr_u
                        (i32.and
                            (i32.load offset=0x6000004 (;ht_stationary_counts;) (i32.shl (local.get $hash) (i32.const 4)))
                            (i32.const 0x003fffff)
                        )
                        (i32.const 10)
                    )
                )
            )
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))

            ;; run model
            (i32.store16 offset=0x0f06000 (;stage_1_probs;)
                (local.get $probs_ptr)
                (call $stretch
                    (i32.and
                        (i32.mul
                            (i32.div_s
                                (i32.const 2048)
                                (i32.add
                                    (i32.load16_s offset=0x600000c (;ht_run_count;) (i32.shl (local.get $hash) (i32.const 4)))
                                    (i32.const 1)
                                )
                            )
                            (i32.add
                                (i32.mul
                                    (i32.load16_s offset=0x600000e (;ht_run_symbol;) (i32.shl (local.get $hash) (i32.const 4)))
                                    (i32.const -2)
                                )
                                (i32.const 1)
                            )
                        )
                        (i32.const 0x0fff)
                    )
                )
            )
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))

            (br_if $update_context_models_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (global.get $num_active_context_models)))
        )

        ;; const model
        (i32.store16 offset=0x0f06000 (;stage_1_probs;)
            (local.get $probs_ptr)
            (i32.const 1024)
        )
        (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))

        ;; match models
        (local.set $i (i32.const 0))
        (loop $update_match_models_loop
            (i32.store16 offset=0x0f06000 (;stage_1_probs;)
                (local.get $probs_ptr)
                (call $stretch (call $match_model_prob (local.get $i)))
            )
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))
            (br_if $update_match_models_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )

        ;; todo: all stage_1_probs can be stretched in a loop instead of every time we write one, except the const model which we should pre-unstretch so it returns the right value
        ;; after stretching

        ;; debug: log all stage_1_probs
        (local.set $i (i32.const 0))
        (loop $debug_stage_1_probs_loop
            (local.set $i (i32.add (local.get $i) (i32.const 2)))
            (br_if $debug_stage_1_probs_loop (i32.lt_u (local.get $i) (local.get $probs_ptr)))
        )

        (local.set $i (i32.const 0))
        (loop $set_stage_1_weight_contexts_loop
            (i32.store offset=0x0f07000 (;stage_1_weight_contexts;) (i32.shl (local.get $i) (i32.const 2))
                (call $history_get (i32.const 0)
                    (i32.sub
                        (i32.sub
                            (call $history_get_byte_history_pos (i32.const 0))
                            (i32.const 1)
                        )
                        (local.get $i)
                    )
                )
            )
            (br_if $set_stage_1_weight_contexts_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 4)))
        )
        (i32.store offset=0x0f07000 (;stage_1_weight_contexts;) (i32.const 16)
            (i32.and (call $history_hash (i32.const 0) (i32.const 0xff)) (i32.const 0xff))
        )
        (i32.store offset=0x0f07000 (;stage_1_weight_contexts;) (i32.const 20) (global.get $bit_history_hash))
        (i32.store offset=0x0f07000 (;stage_1_weight_contexts;) (i32.const 24)
            (i32.or
                (i32.or
                    (i32.shr_s
                        (call $squash
                            (i32.load16_s offset=0x0f06000 (;stage_1_probs;)
                                (i32.shl (i32.sub (global.get $num_model_outputs) (i32.const 1)) (i32.const 1))
                            )
                        )
                        (i32.const 6)
                    )
                    (select (i32.const 64) (i32.const 0)
                        (i32.eq
                            (call $history_get (i32.const 0) (i32.sub (call $history_get_byte_history_pos (i32.const 0)) (i32.const 1)))
                            (call $history_get (i32.const 0) (i32.sub (call $history_get_byte_history_pos (i32.const 0)) (i32.const 2)))
                        )
                    )
                )
                (select (i32.const 128) (i32.const 0)
                    (i32.eq
                        (call $history_get (i32.const 0) (i32.sub (call $history_get_byte_history_pos (i32.const 0)) (i32.const 2)))
                        (call $history_get (i32.const 0) (i32.sub (call $history_get_byte_history_pos (i32.const 0)) (i32.const 3)))
                    )
                )
            )
        )

        (local.set $i (i32.const 0))
        (local.set $probs_ptr (i32.const 0))
        (loop $create_stage_2_probs_loop
            (i32.store16 offset=0x0f08000 (;stage_2_probs;)
                (local.get $probs_ptr)
                (call $mix
                    (i32.const 0x0f06000 (;stage_1_probs;))
                    (call $get_stage_1_weights_ptr (local.get $i))
                    (i32.shr_u (global.get $num_model_outputs) (i32.const 3))
                )
            )

            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))
            (br_if $create_stage_2_probs_loop (i32.lt_u (local.get $probs_ptr) (i32.const 16)))
        )
        ;; debug: log all stage_1_weights
        (local.set $i (i32.const 0))
        (loop $debug_stage_1_weight_contexts_loop
            (local.set $tmp (i32.const 0))
            (loop $debug_inner_s1wcl
                (local.set $tmp (i32.add (local.get $tmp) (i32.const 1)))
                (br_if $debug_inner_s1wcl (i32.lt_u (local.get $tmp) (i32.const 8)))
            )
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br_if $debug_stage_1_weight_contexts_loop (i32.lt_u (local.get $i) (i32.const 8)))
        )
        ;; debug: log all stage_1_weight_contexts
        (local.set $i (i32.const 0))
        (loop $debug_stage_1_weight_contexts_loop
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br_if $debug_stage_1_weight_contexts_loop (i32.lt_u (local.get $i) (i32.const 8)))
        )
        ;; debug: log all stage_2_probs
        (local.set $i (i32.const 0))
        (loop $debug_stage_2_probs_loop
            (local.set $i (i32.add (local.get $i) (i32.const 2)))
            (br_if $debug_stage_2_probs_loop (i32.lt_u (local.get $i) (i32.const 16)))
        )

        (global.set $stage_2_prob
            (call $mix
                (i32.const 0x0f08000 (;stage_2_probs;))
                (i32.add (i32.const 0x0f0d000 (;dis_model_context.stage_2_weights;)) (i32.shl (global.get $dis_model_state) (i32.const 4)))
                (i32.const 1)
            )
        )

        (local.set $prob (global.get $stage_2_prob))

        (local.set $i (i32.const 0))
        (loop $apm_loop
            (local.set $apm_context (call $history_hash (i32.const 0) (i32.sub (i32.shl (i32.const 1) (local.get $i)) (i32.const 1))))
            (local.set $apm_context (call $hash_byte (local.get $apm_context) (global.get $bit_history_hash)))
            (call $apm_stage_set_index (local.get $i) (local.get $apm_context) (local.get $prob))

            (local.set $prob
                (i32.add
                    (local.get $prob)
                    (i32.shr_s
                        (i32.mul
                            (i32.sub
                                (call $stretch (call $apm_stage_prob (local.get $i)))
                                (local.get $prob)
                            )
                            (i32.load8_s offset=0x0f04000 (local.get $i)) ;; apm_mix_weights
                        )
                        (i32.const 4)
                    )
                )
            )
            (br_if $apm_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 3)))
        )

        (local.set $prob (call $squash (local.get $prob)))

        (if (i32.lt_u (local.get $prob) (i32.const 1)) (then (local.set $prob (i32.const 1))))
        (if (i32.gt_u (local.get $prob) (i32.const 4095)) (then (local.set $prob (i32.const 4095))))

        (local.get $prob)
    )

    (func $dm_update (param $byte i32)
        (local $i i64)
        (local $state i32)
        (local $should_remain i32)

        (local.set $state (call $dm_val))
        (call $l_dm (local.get $state) (global.get $dm_opcode) (local.get $byte) (global.get $dm_read_pos) (global.get $dm_write_pos))

        (block $done
            (block $opcode
                (block $prefixed_opcode
                    (block $local_type_count
                        (block $mem_arg_align
                            (block $br_table
                                (block $const_f32_or_f64
                                    (block $leb
                                        (br_table
                                            $opcode
                                            $prefixed_opcode
                                            $leb
                                            $leb
                                            $local_type_count
                                            $leb
                                            $done
                                            $leb
                                            $leb
                                            $const_f32_or_f64
                                            $const_f32_or_f64
                                            $leb
                                            $leb
                                            $leb
                                            $leb
                                            $mem_arg_align
                                            $leb
                                            $leb
                                            $leb
                                            $leb
                                            $br_table
                                            $leb
                                            $leb
                                            $done
                                            $leb
                                            (local.get $state)
                                        )
                                        unreachable
                                    )
                                    ;; leb
                                    (if (call $dm_update_leb (local.get $byte)) (then) (else
                                        (local.set $should_remain (i32.const 1))
                                    ))
                                    br $done
                                )
                                ;; const_f32_or_f64
                                (global.set $dm_float_bytes_left (i32.sub (global.get $dm_float_bytes_left) (i32.const 1)))
                                (if (global.get $dm_float_bytes_left) (then
                                    (local.set $should_remain (i32.const 1))
                                ))
                                br $done
                            )
                            ;; br_table
                            (if (call $dm_update_leb (local.get $byte)) (then
                                (if (i64.ne (global.get $dm_leb_val) (i64.const 0)) (then
                                    (loop $br_table_loop
                                        (call $dm_write (i32.const 21))
                                        (br_if $br_table_loop (i64.lt_u (local.tee $i (i64.add (local.get $i) (i64.const 1))) (global.get $dm_leb_val)))
                                    )
                                ))
                                (call $dm_write (i32.const 21))
                            ))
                            br $done
                        )
                        ;; mem_arg_align
                        (if (call $dm_update_leb (local.get $byte)) (then
                            (if (i64.ge_u (global.get $dm_leb_val) (i64.const 64)) (then
                                (call $dm_write (i32.const 16))
                            ))
                            (call $dm_write (i32.const 17))
                        ))
                        br $done
                    )
                    ;; local_type_count
                    (if (call $dm_update_leb (local.get $byte)) (then
                        (if (i64.ne (global.get $dm_leb_val) (i64.const 0)) (then
                            (loop $local_type_count_loop
                                (call $dm_write (i32.const 5))
                                (call $dm_write (i32.const 6))
                                (br_if $local_type_count_loop (i64.lt_u (local.tee $i (i64.add (local.get $i) (i64.const 1))) (global.get $dm_leb_val)))
                            )
                        ))
                    ))
                    br $done
                )
                ;; prefix_opcode
                (if (call $dm_update_leb (local.get $byte)) (then
                    (if (i32.eq (global.get $dm_opcode) (i32.const 0xfc)) (then
                        (if
                            (i32.or
                                (i32.eq (local.get $byte) (i32.const 0x8))
                                (i32.eq (local.get $byte) (i32.const 0xa))
                            )
                             (then
                                (call $dm_write (i32.const 11))
                                (call $dm_write (i32.const 12))
                        ))
                        (if
                            (i32.or
                                (i32.eq (local.get $byte) (i32.const 0xc))
                                (i32.eq (local.get $byte) (i32.const 0xe))
                            )
                             (then
                                (call $dm_write (i32.const 24))
                                (call $dm_write (i32.const 24))
                        ))
                        (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x09)) (i32.le_u (local.get $byte) (i32.const 0x12)))
                             (then
                                (call $dm_write (i32.const 12))
                        ))
                    ))
                    (if (i32.eq (global.get $dm_opcode) (i32.const 0xfd)) (then
                        (if (i32.or
                                (i32.le_u (local.get $byte) (i32.const 11))
                                (i32.or (i32.eq (local.get $byte) (i32.const 92)) (i32.eq (local.get $byte) (i32.const 93)))
                            )
                             (then
                                (call $dm_write (i32.const 15))
                        ))
                        (if (i32.or (i32.eq (local.get $byte) (i32.const 12)) (i32.eq (local.get $byte) (i32.const 13)))
                             (then
                                (loop $write_vector_byte_loop
                                    (call $dm_write (i32.const 23))
                                    (br_if $write_vector_byte_loop (i64.lt_u (local.tee $i (i64.add (local.get $i) (i64.const 1))) (i64.const 16)))
                                )
                        ))
                        (if
                            (i32.or
                                (i32.and (i32.ge_u (local.get $byte) (i32.const 21)) (i32.le_u (local.get $byte) (i32.const 34)))
                                (i32.and (i32.ge_u (local.get $byte) (i32.const 84)) (i32.le_u (local.get $byte) (i32.const 91)))
                            )
                             (then
                                (call $dm_write (i32.const 15))
                                (call $dm_write (i32.const 22))
                        ))
                    ))
                    (if (i32.eq (global.get $dm_opcode) (i32.const 0xfe)) (then
                        (if (i32.eq (local.get $byte) (i32.const 3))
                             (then
                                (call $dm_write (i32.const 23))
                        ))
                        (if
                            (i32.or
                                (i32.and (i32.ge_u (local.get $byte) (i32.const 0x0)) (i32.le_u (local.get $byte) (i32.const 0x02)))
                                (i32.and (i32.ge_u (local.get $byte) (i32.const 0x10)) (i32.le_u (local.get $byte) (i32.const 0x4e)))
                            )
                             (then
                                (call $dm_write (i32.const 15))
                        ))
                    ))
                ))
                br $done
            )
            ;; opcode
            (global.set $dm_opcode (local.get $byte))

            (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x2)) (i32.le_u (local.get $byte) (i32.const 0x4))) (then
                ;; block/loop/if
                (global.set $dm_depth (i32.add (global.get $dm_depth) (i32.const 1)))
                (call $dm_write (i32.const 19))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0xb)) (then
                ;; end of block
                (if (i32.gt_u (global.get $dm_depth) (i32.const 0)) (then
                    (global.set $dm_depth (i32.sub (global.get $dm_depth) (i32.const 1)))
                ) (else
                    ;; end of function, so prepare for the next one
                    (call $dm_write (i32.const 3))
                    (call $dm_write (i32.const 4))
                ))
            ))
            (if
                (i32.or
                    (i32.and (i32.ge_u (local.get $byte) (i32.const 0xc)) (i32.le_u (local.get $byte) (i32.const 0xd)))
                    (i32.and (i32.ge_u (local.get $byte) (i32.const 0xd5)) (i32.le_u (local.get $byte) (i32.const 0xd6)))
                )
                 (then
                    (call $dm_write (i32.const 21))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0xe)) (then
                (call $dm_write (i32.const 20))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x10)) (then
                (call $dm_write (i32.const 18))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x11)) (then
                (call $dm_write (i32.const 24))
                (call $dm_write (i32.const 24))
            ))
            (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x20)) (i32.le_u (local.get $byte) (i32.const 0x22))) (then
                (call $dm_write (i32.const 13))
            ))
            (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x23)) (i32.le_u (local.get $byte) (i32.const 0x24))) (then
                (call $dm_write (i32.const 14))
            ))
            (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x28)) (i32.le_u (local.get $byte) (i32.const 0x3e))) (then
                (call $dm_write (i32.const 15))
            ))
            (if (i32.and (i32.ge_u (local.get $byte) (i32.const 0x3f)) (i32.le_u (local.get $byte) (i32.const 0x40))) (then
                (call $dm_write (i32.const 12))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x41)) (then
                (call $dm_write (i32.const 7))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x42)) (then
                (call $dm_write (i32.const 8))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x43)) (then
                (call $dm_write (i32.const 9))
                (global.set $dm_float_bytes_left (i32.const 4))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0x44)) (then
                (call $dm_write (i32.const 10))
                (global.set $dm_float_bytes_left (i32.const 8))
            ))
            (if (i32.eq (local.get $byte) (i32.const 0xd0)) (then
                (call $dm_write (i32.const 24))
            ))
            (if
                (i32.or
                    (i32.eq (local.get $byte) (i32.const 0xd2))
                    (i32.and (i32.ge_u (local.get $byte) (i32.const 0xd5)) (i32.le_u (local.get $byte) (i32.const 0xd6)))
                )
                 (then
                    (call $dm_write (i32.const 21))
            ))
        )

        (if (local.get $should_remain) (then) (else
            (global.set $dm_read_pos (i32.and (i32.add (global.get $dm_read_pos) (i32.const 1)) (i32.const 0x3fff)))

            (if (i32.eq (global.get $dm_read_pos) (global.get $dm_write_pos)) (then
                (global.set $dm_read_pos (i32.const 0))
                (global.set $dm_write_pos (i32.const 1))
                (i32.store8 offset=0x0f20000 (i32.const 0) (i32.const 0))
            ))
        ))
    )

    (func $dm_update_leb (param $byte i32) (result i32)
        (global.set $dm_temp_leb_val
            (i64.add (global.get $dm_temp_leb_val) (i64.extend_i32_u (i32.shl (i32.and (local.get $byte) (i32.const 0x7f)) (global.get $dm_leb_shift))))
        )
        (global.set $dm_leb_shift (i32.add (global.get $dm_leb_shift) (i32.const 7)))
        (if (result i32) (i32.ne (i32.and (local.get $byte) (i32.const 0x80)) (i32.const 0x80))
            (then
                (global.set $dm_leb_val (global.get $dm_temp_leb_val))
                (global.set $dm_temp_leb_val (i64.const 0))
                (global.set $dm_leb_shift (i32.const 0))
                (i32.const 1)
            ) (else (i32.const 0))
        )
    )

    (func $dm_write (param $state i32)
        (i32.store8 offset=0x0f20000 (global.get $dm_write_pos) (local.get $state))
        (global.set $dm_write_pos (i32.and (i32.add (global.get $dm_write_pos) (i32.const 1)) (i32.const 0x3fff)))
    )

    (func $dm_val (result i32)
        (i32.load8_u offset=0x0f20000 (global.get $dm_read_pos))
    )

    (func $model_update (export "model_update") (param $bit i32) (param $is_in_code_section i32)
        (local $i i32)
        (local $indirect_prob_index i32)
        (local $indirect_prob i32)
        (local $byte i32)
        (local $hash i32)
        (local $count i32)
        (local $prob i32)
        (local $counts i32)
        (local $counts_zero i32)
        (local $counts_one i32)
        (local $last_bits i32)
        (local $probs_ptr i32)
        (local $history_index i32)

        ;; update context models
        (local.set $i (i32.const 0))
        (loop $update_context_models_loop
            (local.set $hash
                (i32.load offset=0x0f0a000 (;context_model_hashes;) (i32.shl (local.get $i) (i32.const 2)))
            )

            ;; indirect model
            ;; update indirect prob
            (local.set $indirect_prob_index
                (i32.load offset=0x0f0b000 (;context_model_indirect_prob_indices;) (i32.shl (local.get $i) (i32.const 2)))
            )

            (local.set $indirect_prob
                (i32.load16_u
                    offset=0x1ac00000 (;context_indirect_probs;)
                    (i32.add
                        (i32.mul (global.get $dis_model_state) (i32.const 0x540000))
                        (i32.shl (local.get $indirect_prob_index) (i32.const 1))
                    )
                )
            )

            (local.set $indirect_prob
                (i32.and
                    (i32.add
                        (local.get $indirect_prob)
                        (i32.shr_u
                            (i32.sub
                                (i32.shl (local.get $bit) (i32.const 16))
                                (local.get $indirect_prob)
                            )
                            (i32.const 6)
                        )
                    )
                    (i32.const 0xffff)
                )
            )

            (i32.store16
                offset=0x1ac00000 (;context_indirect_probs;)
                (i32.add
                    (i32.mul (global.get $dis_model_state) (i32.const 0x540000))
                    (i32.shl (local.get $indirect_prob_index) (i32.const 1))
                )
                (local.get $indirect_prob)
            )

            ;; update counts
            (local.set $counts
                (i32.load offset=0x6000008 (;ht_indirect_counts;) (i32.shl (local.get $hash) (i32.const 4)))
            )

            (local.set $counts_zero (i32.and (local.get $counts) (i32.const 0x3f)))
            (local.set $counts_one (i32.and (i32.shr_u (local.get $counts) (i32.const 6)) (i32.const 0x3f)))
            (local.set $last_bits (i32.shr_u (local.get $counts) (i32.const 12)))

            (if (local.get $bit)
                (then
                    ;; bit >= 0 (condition other way around from model.rs)

                    (if (i32.lt_u (local.get $counts_one) (i32.const 63)) (then
                        (local.set $counts_one (i32.add (local.get $counts_one) (i32.const 1)))
                    ))
                    (if (i32.gt_u (local.get $counts_zero) (i32.const 9)) (then
                        (local.set $counts_zero (i32.const 9))
                    ))
                )
                (else
                    (if (i32.lt_u (local.get $counts_zero) (i32.const 63)) (then
                        (local.set $counts_zero (i32.add (local.get $counts_zero) (i32.const 1)))
                    ))
                    (if (i32.gt_u (local.get $counts_one) (i32.const 9)) (then
                        (local.set $counts_one (i32.const 9))
                    ))
                )
            )

            (i32.store offset=0x6000008 (;ht_indirect_counts;) (i32.shl (local.get $hash) (i32.const 4))
                (i32.and
                    (i32.or
                        (i32.or
                            (i32.shl (local.get $last_bits) (i32.const 13))
                            (i32.shl (local.get $bit) (i32.const 12))
                        )
                        (i32.or
                            (i32.shl (local.get $counts_one) (i32.const 6))
                            (local.get $counts_zero)
                        )
                    )
                    (i32.const 0xffff)
                )
            )

            ;; stationary model
            (local.set $counts
                (i32.load offset=0x6000004 (;ht_stationary_counts;) (i32.shl (local.get $hash) (i32.const 4)))
            )

            (local.set $prob (i32.and (local.get $counts) (i32.const 0x003fffff)))
            (local.set $count (i32.shr_s (local.get $counts) (i32.const 22)))

            (local.set $prob
                (i32.add
                    (local.get $prob)
                    (i32.div_s
                        (i32.shl
                            (i32.sub
                                (i32.shl (local.get $bit) (i32.const 22))
                                (local.get $prob)
                            )
                            (i32.const 9)
                        )
                        (i32.add (local.get $count) (i32.const 1024))
                    )
                )
            )

            (if (i32.lt_s (local.get $count) (i32.const 256)) (then
                (local.set $count (i32.add (local.get $count) (i32.const 1)))
            ))

            (i32.store offset=0x6000004 (;ht_stationary_counts;) (i32.shl (local.get $hash) (i32.const 4))
                (i32.or
                    (i32.shl (local.get $count) (i32.const 22))
                    (local.get $prob)
                )
            )

            ;; run model
            (local.set $count
                (i32.load16_s offset=0x600000c (;ht_run_counts;) (i32.shl (local.get $hash) (i32.const 4)))
            )
            (if
                (i32.ne
                    (local.get $bit)
                    (i32.load16_u offset=0x600000e (;ht_run_symbol;) (i32.shl (local.get $hash) (i32.const 4)))
                )
                (then
                    (local.set $count (i32.const 0))
                )
            )
            (if (i32.lt_s (local.get $count) (i32.const 1024)) (then
                (local.set $count (i32.add (local.get $count) (i32.const 1)))
            ))
            (i32.store16 offset=0x600000c (;ht_run_counts;) (i32.shl (local.get $hash) (i32.const 4)) (local.get $count))
            (i32.store16 offset=0x600000e (;ht_run_symbol;) (i32.shl (local.get $hash) (i32.const 4)) (local.get $bit))

            (br_if $update_context_models_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (global.get $num_active_context_models)))
        )

        ;; update match models
        (local.set $i (i32.const 0))
        (loop $mm_loop
            (call $match_model_update_bit (local.get $i) (local.get $bit))
            (br_if $mm_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )

        ;; update model weights
        (local.set $i (i32.const 0))
        (loop $update_model_weights_loop
            (call $train
                (i32.const 0x0f06000 (;stage_1_probs;))
                (call $get_stage_1_weights_ptr (local.get $i))
                (i32.shr_s (global.get $num_model_outputs) (i32.const 3))
                (local.get $bit)
                (call $squash (i32.load16_s offset=0x0f08000 (;stage_2_probs;) (local.get $probs_ptr)))
            )

            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))
            (br_if $update_model_weights_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )
        (call $train
            (i32.const 0x0f08000 (;stage_2_probs;))
            (i32.add (i32.const 0x0f0d000 (;dis_model_context.stage_2_weights;)) (i32.shl (global.get $dis_model_state) (i32.const 4)))
            (i32.const 1)
            (local.get $bit)
            (call $squash (global.get $stage_2_prob))
        )

        ;; debug print
        (local.set $i (i32.const 0))
        (loop $debug_print_loop
            (br_if $debug_print_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 2))) (i32.const 16)))
        )

        ;; update apm stages
        (local.set $i (i32.const 0))
        (loop $apm_loop
            (call $apm_stage_update (local.get $i) (local.get $bit))
            (br_if $apm_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 3)))
        )

        (global.set $bit_history
            (i32.or
                (i32.shl (global.get $bit_history) (i32.const 1))
                (local.get $bit)
            )
        )
        (global.set $bit_index (i32.add (global.get $bit_index) (i32.const 1)))

        (if (i32.eq (global.get $bit_index) (i32.const 8))
            (then
                (local.set $byte (global.get $bit_history))
                (call $history_update (i32.const 0) (local.get $byte))
                (if (global.get $dis_model_state) (then
                    (call $history_update (global.get $dis_model_state) (local.get $byte))
                ))

                (global.set $bit_history (i32.const 0))
                (global.set $bit_index (i32.const 0))
                (if (local.get $is_in_code_section) (then
                    (call $dm_update (local.get $byte))
                ))
                (global.set $dis_model_state
                    (select (i32.add (call $dm_val) (i32.const 1)) (i32.const 0) (local.get $is_in_code_section))
                )
                (global.set $num_active_context_models
                    (select (i32.const 42) (i32.const 21) (local.get $is_in_code_section))
                )
                (global.set $num_model_outputs
                    (select (i32.const 138) (i32.const 72) (local.get $is_in_code_section))
                )

                ;; update active context models
                (local.set $i (i32.const 0))
                (loop $update_active_context_models_loop
                    (local.set $history_index
                        (select (i32.const 0) (global.get $dis_model_state) (i32.lt_u (local.get $i) (i32.const 21)))
                    )
                    (local.set $hash
                        (call $history_hash (local.get $history_index) (i32.load8_u offset=0x00c1000 (;byte_masks;) (i32.rem_u (local.get $i) (i32.const 21))))
                    )
                    (local.set $hash (call $hash_byte (local.get $hash) (global.get $dis_model_state)))
                    (local.set $hash (call $hash_byte (local.get $hash) (i32.const 0)))
                    (i32.store offset=0x0f09000 (;context_model_byte_hashes;) (i32.shl (local.get $i) (i32.const 2)) (local.get $hash))
                    (br_if $update_active_context_models_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (global.get $num_active_context_models)))
                )

                ;; update match models
                (local.set $i (i32.const 0))
                (loop $mm_update_byte_loop
                    (call $match_model_update_byte (local.get $i)
                        (i32.sub
                            (i32.shl
                                (i32.const 1)
                                (i32.add (local.get $i) (i32.const 1))
                            )
                            (i32.const 1)
                        )
                    )
                    (br_if $mm_update_byte_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
                )
            )
        )
    )

    (func $decompress (export "decompress") (result i32)
        (local $i i32)
        (local $bit i32)
        (local $marker_bit i32)
        (local $marker_bit_prob i32)
        (local.set $marker_bit_prob (i32.const 2048))

        (global.set $code_section_start (i32.load (global.get $src_ptr)))
        (global.set $code_section_end (i32.load offset=4 (global.get $src_ptr)))
        (global.set $output_size (i32.load offset=8 (global.get $src_ptr)))
        ;;(global.set $output_size (i32.const 5)) ;; hack
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
                    (i32.store8 offset=0x0100000 (global.get $byte_index)
                        (i32.or
                            (i32.shl (i32.load8_u offset=0x0100000 (global.get $byte_index)) (i32.const 1))
                            (local.get $bit)
                        )
                    )
                    (call $model_update (local.get $bit)
                        (i32.and
                            (i32.ge_u (global.get $byte_index) (global.get $code_section_start))
                            (i32.lt_u (global.get $byte_index) (global.get $code_section_end))
                        )
                    )
                    (br_if $decode_byte_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
                )

                (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 1)))
                br $decode_loop
            )
        )

        (global.get $output_size)
    )
)