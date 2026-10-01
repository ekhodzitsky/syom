//! Native-cell qualification: committed fixtures decode, encoder goldens
//! stay byte-exact vs scalar and vs SSE2-only FFT (TASK-98). No remint.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::imdct::{fft_scalar, fft_sse2_only};
use crate::{DecodeOptions, EncodeOptions, decode_with, encode, encode_with};

fn finite_nonempty(bytes: &[u8], label: &str) {
    let pcm = decode_with(bytes, &DecodeOptions::unbounded()).expect(label);
    assert!(!pcm.channels.is_empty(), "{label} channels");
    for (i, ch) in pcm.channels.iter().enumerate() {
        assert!(!ch.is_empty(), "{label} ch{i} empty");
        assert!(ch.iter().all(|x| x.is_finite()), "{label} ch{i} non-finite");
    }
}

#[test]
fn native_cell_decodes_lc_he_ps_multichannel() {
    finite_nonempty(include_bytes!("goldens/sine48.adts"), "lc");
    finite_nonempty(include_bytes!("goldens/he48.adts"), "he");
    finite_nonempty(include_bytes!("goldens/ps48.adts"), "ps");
    finite_nonempty(include_bytes!("goldens/mc51.adts"), "mc51");
}

#[test]
fn native_cell_is_64_bit() {
    assert_eq!(
        std::mem::size_of::<usize>(),
        8,
        "qualified native cells are 64-bit; i686 is unqualified (PLATFORM.md)"
    );
}

fn assert_encode_eq(got: &[u8], golden: &[u8], label: &str) {
    let got = if golden.first() == Some(&0xff) {
        crate::gapless::strip_id3(got)
    } else {
        got
    };
    assert_eq!(
        got, golden,
        "{label} drifted from committed golden — do not remint to hide ISA drift"
    );
}

#[test]
fn encoder_goldens_match_auto_and_scalar_fft() {
    let pcm = crate::encode_tests::lavc_fixture();
    let trans = crate::encode_transient_tests::transient_fixture();
    let look = EncodeOptions::adts().with_lookahead(true);

    let causal = encode(&pcm, 48_000).expect("causal");
    assert_encode_eq(
        &causal,
        include_bytes!("goldens/enc48.adts"),
        "enc48 causal",
    );

    let m4a = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).expect("m4a");
    assert_encode_eq(&m4a, include_bytes!("goldens/enc48m.m4a"), "enc48m");

    let click = encode(&trans, 48_000).expect("transient");
    assert_encode_eq(&click, include_bytes!("goldens/enc48t.adts"), "enc48t");

    let ahead = encode_with(&trans, 48_000, &look).expect("lookahead");
    assert_encode_eq(&ahead, include_bytes!("goldens/enc48l.adts"), "enc48l");

    let _g = fft_scalar();
    assert_encode_eq(
        &encode(&pcm, 48_000).expect("causal-scalar"),
        &causal,
        "enc48 scalar vs auto",
    );
    assert_encode_eq(
        &encode_with(&pcm, 48_000, &EncodeOptions::m4a()).expect("m4a-scalar"),
        &m4a,
        "enc48m scalar vs auto",
    );
    assert_encode_eq(
        &encode(&trans, 48_000).expect("t-scalar"),
        &click,
        "enc48t scalar vs auto",
    );
    assert_encode_eq(
        &encode_with(&trans, 48_000, &look).expect("l-scalar"),
        &ahead,
        "enc48l scalar vs auto",
    );
}

/// 64 kbps is not a committed golden; scalar vs auto still must match (rate mode).
#[test]
fn encoder_64k_scalar_matches_auto() {
    let pcm = crate::encode_tests::lavc_fixture();
    let opts = EncodeOptions::adts().with_bitrate_bps(64_000);
    let auto = encode_with(&pcm, 48_000, &opts).expect("64k-auto");
    let _g = fft_scalar();
    let sc = encode_with(&pcm, 48_000, &opts).expect("64k-scalar");
    assert_eq!(sc, auto, "64k scalar FFT drifted from auto SIMD");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn encoder_goldens_match_sse2_only_fft() {
    assert!(
        std::is_x86_feature_detected!("sse2"),
        "x86_64 minimum ISA is SSE2"
    );
    let pcm = crate::encode_tests::lavc_fixture();
    let trans = crate::encode_transient_tests::transient_fixture();
    let _g = fft_sse2_only();
    assert_encode_eq(
        &encode(&pcm, 48_000).expect("sse2-causal"),
        include_bytes!("goldens/enc48.adts"),
        "enc48 sse2-only",
    );
    assert_encode_eq(
        &encode(&trans, 48_000).expect("sse2-t"),
        include_bytes!("goldens/enc48t.adts"),
        "enc48t sse2-only",
    );
}

#[test]
fn records_native_cell() {
    eprintln!(
        "TASK-98 cell arch={} os={} ptr={} sse2={} avx={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        8 * std::mem::size_of::<usize>(),
        cfg!(target_feature = "sse2"),
        {
            #[cfg(target_arch = "x86_64")]
            {
                std::is_x86_feature_detected!("avx")
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                false
            }
        }
    );
    assert!(!std::env::consts::ARCH.is_empty());
}
