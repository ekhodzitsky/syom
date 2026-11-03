//! Existing metadata-cap parity with explicit presentation-proof scope.
use super::*;

#[test]
fn push_one_feed_uses_the_metadata_cap_after_sniff() -> Result<()> {
    // The resident cap widens to the metadata budget only once the bytes
    // sniff as fMP4. A single feed that starts unsniffed must still accept
    // a moov above the ADTS buffered cap (here 256) and match one-shot.
    let opts = DecodeOptions::audio().with_memory(MemoryBudgets {
        max_buffered_input_bytes: 256,
        ..MemoryBudgets::default()
    });
    let (info, pcm) = collect(LC, &opts)?;
    let (pinfo, ppcm) = push(LC, &opts, LC.len())?;
    // PCM/tallies match; the streaming path cannot certify a complete lifetime
    // fragment index, whereas the slice path has examined its entire index.
    assert_eq!(
        pinfo.presentation_completion,
        crate::PresentationCompletion::Unbounded {
            indexed_end_reached: false
        }
    );
    assert_eq!(
        info.presentation_completion,
        crate::PresentationCompletion::Unbounded {
            indexed_end_reached: true
        }
    );
    assert_eq!(
        StreamInfo {
            presentation_completion: info.presentation_completion,
            ..pinfo
        },
        info
    );
    assert_eq!(ppcm, pcm);
    Ok(())
}
