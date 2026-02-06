(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_u32" (func $log_u32 (param i32)))

    (import "host" "l_i32" (func $l_i32 (param i32)))
    (import "host" "l_u32" (func $l_u32 (param i32)))
    (import "host" "l_u32_excl" (func $l_u32_excl (param i32)))
    (import "host" "l_x32" (func $l_x32 (param i32)))

    (global $src_ptr (mut i32) (i32.const 0))
    (global $code_section_start (mut i32) (i32.const 0))
    (global $code_section_end (mut i32) (i32.const 0))
    (global $output_size (mut i32) (i32.const 0))
    (global $byte_index (mut i32) (i32.const 0))

    ;; range decoder state
    (global $rd_code (mut i32) (i32.const 0))
    (global $rd_range (mut i32) (i32.const -1))

    ;; model state
    ;;dis_model
    (global $dis_model_state (mut i32) (i32.const 0))
    ;;hash_table
    ;;dis_model_contexts
    ;;context_model_byte_hashes
    ;;context_model_hashes
    ;;context_model_indirect_prob_indices
    (global $stage_2_prob (mut i32) (i32.const 0))
    (global $bit_history (mut i32) (i32.const 0))
    (global $bit_index (mut i32) (i32.const 0))
    (global $bit_history_hash (mut i32) (i32.const 0))
    (global $num_active_context_models (mut i32) (i32.const 21))
    ;;num_model_outputs

    ;; offsets:
    ;; stretch_tab = 0x00d0000
    ;; apm_tabs = (we have 3 of them)
    ;;;; the first tab is from 0x2200000, then 02500000, then 0x2800000
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
    ;; for every dis_model (max 32 of them for now, increase this if we have more DisModelStates + 1):
    ;;;; apm_indices = 0x0f02000
    ;;;; apm_weights = 0x0f03000
    ;;;; byte_history = 0x3000000..0x5000000 (length per history = 0x100000, 32 different histories, bytes are interleaved)
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

    (memory (export "memory") 8192) ;; bomb that tree line about 512mb back

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

    (func $model_init (export "model_init")
        (local $i i32)
        (local $j i32)
        (local $x i32)
        (local $pi i32)
        (local $apm_tab_index i32)
        (local $apm_tab_value i32)

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
                (i32.store16 offset=0x2200000 (local.get $apm_tab_index) (local.get $apm_tab_value))
                (i32.store16 offset=0x2500000 (local.get $apm_tab_index) (local.get $apm_tab_value))
                (i32.store16 offset=0x2800000 (local.get $apm_tab_index) (local.get $apm_tab_value))

                (br_if $apm_tab_inner_loop (i32.lt_u (local.tee $x (i32.add (local.get $x) (i32.const 1))) (i32.const 17)))
            )
            (br_if $apm_tab_outer_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 0x10000)))
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
        (i32.add (i32.const 0x2200000) (i32.mul (i32.const 0x300000) (local.get $apm_index))) ;; apm_tab
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

        ;;(call $l_u32 (local.get $prob))
        ;;(call $l_u32 (i32.load offset=0x0f02000 (call $get_apm_data_offset (local.get $apm_index))))
        ;;(call $l_u32 (i32.load offset=0x0f03000 (call $get_apm_data_offset (local.get $apm_index))))
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
        (i32.load offset=0x00f1000 (i32.shl (local.get $history_idx) (i32.const 2)))
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

        (i32.store offset=0x00f1000
            (i32.shl (local.get $history_idx) (i32.const 2))
            (i32.and
                (i32.add (local.get $byte_history_pos) (i32.const 1))
                (i32.const 0x00fffff)
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

    (func $model_prob (export "model_prob") (result i32)
        (local $i i32)
        (local $prob i32)
        (local $apm_context i32)
        (local $probs_ptr i32)

        (global.set $bit_history_hash
            (i32.or
                (i32.shl (i32.const 1) (global.get $bit_index))
                (global.get $bit_history)
            )
        )

        (; update match models ;)
        (local.set $i (i32.const 0))
        (loop $update_match_models_loop
            (i32.store16 offset=0x0f06000 (;stage_1_probs;)
                (local.get $probs_ptr)
                (call $stretch (call $match_model_prob (local.get $i)))
            )
            (local.set $probs_ptr (i32.add (local.get $probs_ptr) (i32.const 2)))
            (br_if $update_match_models_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )

        (; debug: log all stage_1_probs ;)
        (;local.set $i (i32.const 0))
        (loop $debug_stage_1_probs_loop
            (call $l_i32 (i32.load16_s offset=0x0f06000 (local.get $i)))
            (local.set $i (i32.add (local.get $i) (i32.const 2)))
            (br_if $debug_stage_1_probs_loop (i32.lt_u (local.get $i) (local.get $probs_ptr)))
        ;)

        (global.set $stage_2_prob (i32.const 0)) ;; hack
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

    (func $model_update (export "model_update") (param $bit i32)
        (local $i i32)

        (; update match models ;)
        (local.set $i (i32.const 0))
        (loop $mm_loop
            (call $match_model_update_bit (local.get $i) (local.get $bit))
            (br_if $mm_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
        )

        (; update apm stages ;)
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
                (call $history_update (i32.const 0) (global.get $bit_history))
                (global.set $bit_history (i32.const 0))
                (global.set $bit_index (i32.const 0))

                (; update match models ;)
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

    (func (export "decompress")
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
                    (call $model_update (local.get $bit))
                    (br_if $decode_byte_loop (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 8)))
                )

                (global.set $byte_index (i32.add (global.get $byte_index) (i32.const 1)))
                br $decode_loop
            )
        )
    )
)