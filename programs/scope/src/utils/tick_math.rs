// use uint::construct_uint;
// use ratex_contracts::math::bn::U256;
use raydium_amm_v3::libraries::U256;

// Define a U256 type
// construct_uint! {
//     pub struct U256(4);
// }

pub fn tick_index_to_sqrt_price_x64(tick_index: i32) -> U256 {
    if tick_index > 0 {
        tick_index_to_sqrt_price_positive(tick_index)
    } else {
        tick_index_to_sqrt_price_negative(tick_index)
    }
}

fn tick_index_to_sqrt_price_positive(tick: i32) -> U256 {
    let mut ratio: U256;

    if (tick & 1) != 0 {
        ratio = U256::from_dec_str("79232123823359799118286999567").unwrap();
    } else {
        ratio = U256::from_dec_str("79228162514264337593543950336").unwrap();
    }

    if (tick & 2) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79236085330515764027303304731").unwrap(), 96, 256,
        );
    }
    if (tick & 4) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79244008939048815603706035061").unwrap(), 96, 256,
        );
    }
    if (tick & 8) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79259858533276714757314932305").unwrap(), 96, 256,
        );
    }
    if (tick & 16) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79291567232598584799939703904").unwrap(), 96, 256,
        );
    }
    if (tick & 32) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79355022692464371645785046466").unwrap(), 96, 256,
        );
    }
    if (tick & 64) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79482085999252804386437311141").unwrap(), 96, 256,
        );
    }
    if (tick & 128) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("79736823300114093921829183326").unwrap(), 96, 256,
        );
    }
    if (tick & 256) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("80248749790819932309965073892").unwrap(), 96, 256,
        );
    }
    if (tick & 512) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("81282483887344747381513967011").unwrap(), 96, 256,
        );
    }
    if (tick & 1024) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("83390072131320151908154831281").unwrap(), 96, 256,
        );
    }
    if (tick & 2048) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("87770609709833776024991924138").unwrap(), 96, 256,
        );
    }
    if (tick & 4096) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("97234110755111693312479820773").unwrap(), 96, 256,
        );
    }
    if (tick & 8192) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("119332217159966728226237229890").unwrap(), 96, 256,
        );
    }
    if (tick & 16384) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("179736315981702064433883588727").unwrap(), 96, 256,
        );
    }
    if (tick & 32768) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("407748233172238350107850275304").unwrap(), 96, 256,
        );
    }
    if (tick & 65536) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("2098478828474011932436660412517").unwrap(), 96, 256,
        );
    }
    if (tick & 131072) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("55581415166113811149459800483533").unwrap(), 96, 256,
        );
    }
    if (tick & 262144) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("38992368544603139932233054999993551").unwrap(), 96, 256,
        );
    }

    signed_shift_right(ratio, 32, 256)
}

fn tick_index_to_sqrt_price_negative(tick_index: i32) -> U256 {
    let tick = tick_index.abs();
    let mut ratio: U256;

    if (tick & 1) != 0 {
        ratio = U256::from_dec_str("18445821805675392311").unwrap();
    } else {
        ratio = U256::from_dec_str("18446744073709551616").unwrap();
    }

    if (tick & 2) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18444899583751176498").unwrap(), 64, 256,
        );
    }
    if (tick & 4) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18443055278223354162").unwrap(), 64, 256,
        );
    }
    if (tick & 8) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18439367220385604838").unwrap(), 64, 256,
        );
    }
    if (tick & 16) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18431993317065449817").unwrap(), 64, 256,
        );
    }
    if (tick & 32) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18417254355718160513").unwrap(), 64, 256,
        );
    }
    if (tick & 64) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18387811781193591352").unwrap(), 64, 256,
        );
    }
    if (tick & 128) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18329067761203520168").unwrap(), 64, 256,
        );
    }
    if (tick & 256) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("18212142134806087854").unwrap(), 64, 256,
        );
    }
    if (tick & 512) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("17980523815641551639").unwrap(), 64, 256,
        );
    }
    if (tick & 1024) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("17526086738831147013").unwrap(), 64, 256,
        );
    }
    if (tick & 2048) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("16651378430235024244").unwrap(), 64, 256,
        );
    }
    if (tick & 4096) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("15030750278693429944").unwrap(), 64, 256,
        );
    }
    if (tick & 8192) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("12247334978882834399").unwrap(), 64, 256,
        );
    }
    if (tick & 16384) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("8131365268884726200").unwrap(), 64, 256,
        );
    }
    if (tick & 32768) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("3584323654723342297").unwrap(), 64, 256,
        );
    }
    if (tick & 65536) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("696457651847595233").unwrap(), 64, 256,
        );
    }
    if (tick & 131072) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("26294789957452057").unwrap(), 64, 256,
        );
    }
    if (tick & 262144) != 0 {
        ratio = signed_shift_right(
            ratio * U256::from_dec_str("37481735321082").unwrap(), 64, 256,
        );
    }

    ratio
}

// fn signed_shift_left(n0: U256, shift_by: u32, bit_width: u32) -> U256 {
//     let twos_n0 = n0 << shift_by;
//     let mask = (U256::from(1u64) << bit_width) - U256::from(1u64);
//     twos_n0 & mask
// }

fn signed_shift_right(n0: U256, shift_by: u32, bit_width: u32) -> U256 {
    let twos_n0 = n0 >> shift_by;
    let mask = (U256::from(1u64) << (bit_width - shift_by)) - U256::from(1u64);
    twos_n0 & mask
}
