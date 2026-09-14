//! `AacError` Display / source / format-class.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{AacError, Result};
use std::io;

#[test]
fn display_and_source_cover_variants() -> Result<()> {
    let io = AacError::from(io::Error::other("x"));
    assert!(io.to_string().contains('x'));
    assert!(std::error::Error::source(&io).is_some());
    assert_eq!(AacError::NotAac.to_string(), "Unsupported audio format");
    assert!(AacError::sample_rate(8, 4).to_string().contains("8Hz"));
    assert!(AacError::too_long(9.0, 1.0).to_string().contains("9s"));
    assert_eq!(AacError::format("f").to_string(), "f");
    assert_eq!(AacError::decode("d").to_string(), "d");
    assert!(
        AacError::invalid_limits("NaN")
            .to_string()
            .contains("invalid decode limits")
    );
    assert!(
        AacError::limit(crate::BudgetKind::Output, 9, 4)
            .to_string()
            .contains("output")
    );
    assert!(AacError::NotAac.is_format_class());
    assert!(AacError::format("f").is_format_class());
    assert!(!AacError::decode("d").is_format_class());
    Ok(())
}

#[test]
fn engine_error_display_covers_variants() {
    use crate::engine::error::Error;
    let all = [
        Error::UnexpectedEnd,
        Error::AdtsSyncNotFound,
        Error::AdtsLayerNonZero,
        Error::AdtsReservedSampleRateIndex,
        Error::AdtsFrameLengthTooSmall,
        Error::UnsupportedAot(5),
        Error::UnsupportedFrameLength,
        Error::UnsupportedSampleRateIndex(15),
        Error::IcsInfoInvalid,
        Error::SectionDataOverrun,
        Error::InvalidCodebook(12),
        Error::HuffmanInvalid,
        Error::SpectrumInvalid,
        Error::FilterbankInvalid,
        Error::Format("x"),
        Error::LoasSyncInvalid,
        Error::LatmAudioMuxVersionAReserved,
        Error::LatmConfigOutOfRange,
        Error::LatmCrcMismatch,
        Error::LatmNoPreviousMuxConfig,
        Error::LatmUnsupportedFrameLengthType,
        Error::SbrQmfInvalid,
        Error::SbrFreqBandInvalid,
        Error::SbrGridInvalid,
        Error::SbrHuffInvalid,
        Error::PsDataInvalid,
        Error::UnsupportedExtensionSbr(0xd),
        Error::UnsupportedExtensionType(2),
        Error::ExtensionPayloadInvalid,
        Error::UnsupportedCce,
    ];
    for e in all {
        assert!(e.to_string().starts_with("aac:"));
        let _ = std::error::Error::source(&e);
    }
}

#[test]
fn engine_errors_map_to_public_classes() {
    use crate::engine::error::Error;
    use crate::{MalformedKind, UnsupportedFeature};
    assert!(matches!(
        AacError::from(Error::UnexpectedEnd),
        AacError::Truncated { at: None }
    ));
    assert!(matches!(
        AacError::from(Error::UnsupportedAot(1)),
        AacError::Unsupported(UnsupportedFeature::AudioObjectType(1))
    ));
    assert!(matches!(
        AacError::from(Error::UnsupportedFrameLength),
        AacError::Unsupported(UnsupportedFeature::FrameLength960)
    ));
    assert!(matches!(
        AacError::from(Error::AdtsCrcMismatch),
        AacError::Malformed(MalformedKind::AdtsCrc)
    ));
    assert!(matches!(
        AacError::from(Error::HuffmanInvalid),
        AacError::Malformed(MalformedKind::Huffman)
    ));
    assert!(matches!(
        AacError::from(Error::LatmAudioMuxVersionAReserved),
        AacError::Unsupported(UnsupportedFeature::LatmVersionA)
    ));
    assert!(matches!(
        AacError::from(Error::Format("x")),
        AacError::Decode(_)
    ));
}

#[test]
fn public_decode_encode_match_without_strings() {
    use crate::{
        DecodeOptions, Decoder, EncodeOptions, Encoder, LifecycleState, MalformedKind, PcmReject,
        UnsupportedFeature, decode, decode_with, encode,
    };
    assert!(matches!(decode(&[]).unwrap_err(), AacError::NotAac));
    let too_small = include_bytes!("../corpus/fuzz/adts-len-too-small.bin");
    assert!(matches!(
        decode(too_small).unwrap_err(),
        AacError::Malformed(MalformedKind::AdtsFrameLength)
    ));
    let srate = include_bytes!("../corpus/fuzz/adts-reserved-srate.bin");
    assert!(matches!(
        decode(srate).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::SampleRateIndex(_))
    ));
    // Valid ADTS header, body truncated: Truncated, not NotAac.
    let mut trunc = include_bytes!("goldens/sine48.adts")[..7].to_vec();
    trunc[3] = (trunc[3] & 0xFC) | 0x01; // claim a long frame
    trunc[4] = 0x00;
    trunc[5] &= 0x1F;
    assert!(matches!(
        decode_with(&trunc, &DecodeOptions::unbounded()).unwrap_err(),
        AacError::Truncated { .. }
    ));
    let tiny = DecodeOptions::speech().with_memory(crate::MemoryBudgets {
        max_output_bytes: 64,
        ..crate::MemoryBudgets::default()
    });
    assert!(matches!(
        decode_with(include_bytes!("goldens/sine48.adts"), &tiny).unwrap_err(),
        AacError::Limit { .. }
    ));
    assert!(matches!(
        encode(&[vec![f32::NAN; 64]], 48_000).unwrap_err(),
        AacError::InvalidPcm(PcmReject::NonFinite)
    ));
    let mut dec = Decoder::new(DecodeOptions::speech());
    dec.feed(include_bytes!("goldens/sine48.adts"), |_| Ok(()))
        .unwrap();
    dec.finish(|_| Ok(())).unwrap();
    assert!(matches!(
        dec.feed(&[], |_| Ok(())).unwrap_err(),
        AacError::Lifecycle {
            state: LifecycleState::Finished
        }
    ));
    let mut main = include_bytes!("goldens/sine48.adts").to_vec();
    main[2] &= 0x3F; // profile Main (0)
    let e = decode_with(&main, &DecodeOptions::unbounded()).unwrap_err();
    assert!(
        matches!(
            e,
            AacError::Unsupported(UnsupportedFeature::AudioObjectType(1))
        ),
        "Main AOT: {e:?}"
    );
    match Encoder::new(48_000, 1, &EncodeOptions::m4a()) {
        Err(AacError::Unsupported(UnsupportedFeature::EncodeM4aStreaming)) => {}
        Err(e) => panic!("expected EncodeM4aStreaming, got {e:?}"),
        Ok(_) => panic!("M4A push encoder must error"),
    }
    let io = AacError::from(std::io::Error::other("x"));
    assert!(std::error::Error::source(&io).is_some());
}
