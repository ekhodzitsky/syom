use std::io::Cursor;
use syom::{DecodeOptions, EncodeOptions, M4aSeek};
#[test]
fn bounded_presentation_must_not_finish_early() {
    let mut data = syom::encode_with(&[vec![0.1f32; 1]], 48000, &EncodeOptions::m4a()).unwrap();
    let at = data.windows(4).position(|w| w == b"elst").unwrap();
    data[at + 12..at + 16].copy_from_slice(&48000u32.to_be_bytes());
    let opts = DecodeOptions::speech();
    let slice = syom::decode_streaming(&data, &opts, |_| Ok(())).is_err();
    let reader = syom::decode_seek_streaming(Cursor::new(&data), &opts, |_| Ok(())).is_err();
    let mut source = M4aSeek::open(Cursor::new(&data), opts).unwrap();
    let seek = source.decode(|_| Ok(())).is_err();
    assert!(
        slice && reader && seek,
        "strict EOF refusal: slice={slice} reader={reader} seek={seek}"
    );
}
