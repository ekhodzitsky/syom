//! Synthetic avc1 + PCM MP4 (kover `write.rs` layout). Tests only.

use crate::error::{SyomError, media};

pub(crate) fn write_mp4(video: &[u8], audio: &[u8]) -> Result<Vec<u8>, SyomError> {
    write_pcm_mp4(video, audio, *b"sowt", 1, 16, 16_000)
}

pub(crate) fn write_pcm_mp4(
    video: &[u8],
    audio: &[u8],
    format: [u8; 4],
    channels: u16,
    bits: u16,
    rate: u32,
) -> Result<Vec<u8>, SyomError> {
    if video.is_empty() {
        return Err(media("empty video"));
    }
    let spec = AudioSpec {
        format,
        channels,
        bits,
        rate,
    };
    let ftyp = wrap(b"ftyp", &{
        let mut p = Vec::new();
        push_tag(&mut p, b"isom");
        push_u32(&mut p, 0);
        push_tag(&mut p, b"isom");
        push_tag(&mut p, b"mp41");
        p
    });
    let sized = wrap(
        b"moov",
        &moov_payload_at(0, 0, video.len() as u32, audio.len() as u32, spec)?,
    );
    let mdat_header = 8u32
        .checked_add(video.len() as u32)
        .and_then(|n| n.checked_add(audio.len() as u32))
        .ok_or_else(|| media("mdat"))?;
    let video_off = u32::try_from(ftyp.len().saturating_add(sized.len()).saturating_add(8))
        .map_err(|_| media("offset"))?;
    let audio_off = video_off
        .checked_add(video.len() as u32)
        .ok_or_else(|| media("offset"))?;
    let moov = wrap(
        b"moov",
        &moov_payload_at(
            video_off,
            audio_off,
            video.len() as u32,
            audio.len() as u32,
            spec,
        )?,
    );
    let mut file = Vec::new();
    file.extend_from_slice(&ftyp);
    file.extend_from_slice(&moov);
    push_u32(&mut file, mdat_header);
    push_tag(&mut file, b"mdat");
    file.extend_from_slice(video);
    file.extend_from_slice(audio);
    Ok(file)
}

pub(crate) fn patch_fourcc(mp4: &mut [u8], from: &[u8; 4], to: &[u8; 4]) -> Result<(), SyomError> {
    let at = mp4
        .windows(4)
        .position(|w| w == from)
        .ok_or_else(|| media("fourcc"))?;
    let end = at.checked_add(4).ok_or_else(|| media("fourcc"))?;
    let slot = mp4.get_mut(at..end).ok_or_else(|| media("fourcc"))?;
    slot.copy_from_slice(to);
    Ok(())
}

#[derive(Clone, Copy)]
struct AudioSpec {
    format: [u8; 4],
    channels: u16,
    bits: u16,
    rate: u32,
}

fn moov_payload_at(
    video_off: u32,
    audio_off: u32,
    video_size: u32,
    audio_size: u32,
    spec: AudioSpec,
) -> Result<Vec<u8>, SyomError> {
    let mut p = Vec::new();
    p.extend_from_slice(&mvhd());
    p.extend_from_slice(&trak(1, b"vide", video_off, video_size, None)?);
    p.extend_from_slice(&trak(2, b"soun", audio_off, audio_size, Some(spec))?);
    Ok(p)
}

fn mvhd() -> Vec<u8> {
    let mut p = vec![0u8; 4];
    p.extend_from_slice(&[0u8; 8]);
    push_u32(&mut p, 1000);
    push_u32(&mut p, 1000);
    push_u32(&mut p, 0x00010000);
    push_u16(&mut p, 0x0100);
    p.extend_from_slice(&[0u8; 10]);
    identity_matrix(&mut p);
    p.extend_from_slice(&[0u8; 24]);
    push_u32(&mut p, 3);
    wrap(b"mvhd", &p)
}

fn trak(
    id: u32,
    handler: &[u8; 4],
    offset: u32,
    sample_size: u32,
    spec: Option<AudioSpec>,
) -> Result<Vec<u8>, SyomError> {
    let mut p = Vec::new();
    p.extend_from_slice(&tkhd(id, spec.is_none()));
    p.extend_from_slice(&mdia(handler, offset, sample_size, spec)?);
    Ok(wrap(b"trak", &p))
}

fn tkhd(id: u32, video: bool) -> Vec<u8> {
    let mut p = Vec::from([0u8, 0, 0, 3]);
    p.extend_from_slice(&[0u8; 8]);
    push_u32(&mut p, id);
    push_u32(&mut p, 0);
    push_u32(&mut p, 1000);
    p.extend_from_slice(&[0u8; 8]);
    p.extend_from_slice(&[0u8; 8]);
    identity_matrix(&mut p);
    if video {
        push_u32(&mut p, 320 << 16);
        push_u32(&mut p, 180 << 16);
    } else {
        push_u32(&mut p, 0);
        push_u32(&mut p, 0);
    }
    wrap(b"tkhd", &p)
}

fn mdia(
    handler: &[u8; 4],
    offset: u32,
    sample_size: u32,
    spec: Option<AudioSpec>,
) -> Result<Vec<u8>, SyomError> {
    let mut p = Vec::new();
    p.extend_from_slice(&mdhd());
    p.extend_from_slice(&hdlr(handler));
    p.extend_from_slice(&minf(offset, sample_size, spec)?);
    Ok(wrap(b"mdia", &p))
}

fn mdhd() -> Vec<u8> {
    let mut p = vec![0u8; 4];
    p.extend_from_slice(&[0u8; 8]);
    push_u32(&mut p, 1000);
    push_u32(&mut p, 1000);
    push_u32(&mut p, 0x55c40000);
    wrap(b"mdhd", &p)
}

fn hdlr(handler: &[u8; 4]) -> Vec<u8> {
    let mut p = vec![0u8; 8];
    push_tag(&mut p, handler);
    p.extend_from_slice(&[0u8; 12]);
    p.extend_from_slice(b"syom\0");
    wrap(b"hdlr", &p)
}

fn minf(offset: u32, sample_size: u32, spec: Option<AudioSpec>) -> Result<Vec<u8>, SyomError> {
    let mut p = Vec::new();
    p.extend_from_slice(&if spec.is_none() {
        wrap(b"vmhd", &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0])
    } else {
        wrap(b"smhd", &[0, 0, 0, 0, 0, 0, 0, 0])
    });
    p.extend_from_slice(&dinf());
    p.extend_from_slice(&stbl(offset, sample_size, spec)?);
    Ok(wrap(b"minf", &p))
}

fn dinf() -> Vec<u8> {
    let url = wrap(b"url ", &[0, 0, 0, 1]);
    let mut dref_p = vec![0u8, 0, 0, 0];
    push_u32(&mut dref_p, 1);
    dref_p.extend_from_slice(&url);
    wrap(b"dinf", &wrap(b"dref", &dref_p))
}

fn stbl(offset: u32, sample_size: u32, spec: Option<AudioSpec>) -> Result<Vec<u8>, SyomError> {
    let mut p = Vec::new();
    p.extend_from_slice(&stsd(spec)?);
    p.extend_from_slice(&stts());
    p.extend_from_slice(&stsc());
    p.extend_from_slice(&stsz(sample_size));
    p.extend_from_slice(&stco(offset));
    Ok(wrap(b"stbl", &p))
}

fn stsd(spec: Option<AudioSpec>) -> Result<Vec<u8>, SyomError> {
    let mut p = vec![0u8, 0, 0, 0];
    push_u32(&mut p, 1);
    p.extend_from_slice(&match spec {
        Some(spec) => audio_entry(spec),
        None => avc1(),
    });
    Ok(wrap(b"stsd", &p))
}

fn avc1() -> Vec<u8> {
    let mut p = vec![0u8; 6];
    push_u16(&mut p, 1);
    p.extend_from_slice(&[0u8; 16]);
    push_u16(&mut p, 320);
    push_u16(&mut p, 180);
    push_u32(&mut p, 0x00480000);
    push_u32(&mut p, 0x00480000);
    push_u32(&mut p, 0);
    push_u16(&mut p, 1);
    p.extend_from_slice(&[0u8; 32]);
    push_u16(&mut p, 0x0018);
    push_u16(&mut p, 0xffff);
    wrap(b"avc1", &p)
}

fn audio_entry(spec: AudioSpec) -> Vec<u8> {
    let mut p = vec![0u8; 6];
    push_u16(&mut p, 1);
    p.extend_from_slice(&[0u8; 8]);
    push_u16(&mut p, spec.channels);
    push_u16(&mut p, spec.bits);
    push_u16(&mut p, 0);
    push_u16(&mut p, 0);
    push_u32(&mut p, spec.rate << 16);
    wrap(&spec.format, &p)
}

fn stts() -> Vec<u8> {
    let mut p = vec![0u8, 0, 0, 0];
    push_u32(&mut p, 1);
    push_u32(&mut p, 1);
    push_u32(&mut p, 1000);
    wrap(b"stts", &p)
}

fn stsc() -> Vec<u8> {
    let mut p = vec![0u8, 0, 0, 0];
    push_u32(&mut p, 1);
    push_u32(&mut p, 1);
    push_u32(&mut p, 1);
    push_u32(&mut p, 1);
    wrap(b"stsc", &p)
}

fn stsz(sample_size: u32) -> Vec<u8> {
    let mut p = vec![0u8, 0, 0, 0];
    push_u32(&mut p, 0);
    push_u32(&mut p, 1);
    push_u32(&mut p, sample_size);
    wrap(b"stsz", &p)
}

fn stco(offset: u32) -> Vec<u8> {
    let mut p = vec![0u8, 0, 0, 0];
    push_u32(&mut p, 1);
    push_u32(&mut p, offset);
    wrap(b"stco", &p)
}

fn identity_matrix(p: &mut Vec<u8>) {
    push_u32(p, 0x00010000);
    push_u32(p, 0);
    push_u32(p, 0);
    push_u32(p, 0);
    push_u32(p, 0x00010000);
    push_u32(p, 0);
    push_u32(p, 0);
    push_u32(p, 0);
    push_u32(p, 0x40000000);
}

fn wrap(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = 8u32.saturating_add(payload.len() as u32);
    let mut out = Vec::with_capacity(size as usize);
    push_u32(&mut out, size);
    push_tag(&mut out, tag);
    out.extend_from_slice(payload);
    out
}

fn push_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn push_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn push_tag(buf: &mut Vec<u8>, tag: &[u8; 4]) {
    buf.extend_from_slice(tag);
}
