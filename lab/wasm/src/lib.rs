//! TASK-100: the smallest host integration of syom on
//! `wasm32-unknown-unknown` — no bindgen, no dependency, a C ABI over
//! linear memory.
//!
//! Ownership: the host asks for a buffer ([`syom_alloc`]), writes input
//! bytes or f32 PCM into it, calls an operation, reads the 48-byte result
//! block at [`syom_result`], and frees what it allocated. The module
//! never touches a filesystem, clock or thread (`syom::read` / `write`
//! exist but have nothing to open here).

use syom::{decode_streaming, encode_with, DecodeOptions, EncodeContainer, EncodeOptions};

/// Result block: `[status, channels, rate, frames, samples_lo, samples_hi,
/// hash_lo, hash_hi, out_ptr, out_len, first_frame_samples, reserved]`.
static mut RESULT: [u32; 12] = [0; 12];
static mut OUT: Vec<u8> = Vec::new();

#[no_mangle]
pub extern "C" fn syom_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` / `len` must come from [`syom_alloc`] with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn syom_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn syom_result() -> *const u32 {
    std::ptr::addr_of!(RESULT).cast()
}

fn fnv(h: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(h, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

pub const FNV_SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// Streaming decode summary: (status, channels, rate, frames, samples,
/// FNV-1a of every f32 bit pattern in frame order, first frame's samples).
pub fn decode_summary(input: &[u8]) -> [u64; 7] {
    let (mut ch, mut rate, mut frames, mut samples, mut hash, mut first) = (0u64, 0u64, 0u64, 0u64, FNV_SEED, 0u64);
    let r = decode_streaming(input, &DecodeOptions::unbounded(), |f| {
        ch = f.planar.len() as u64;
        rate = u64::from(f.sample_rate);
        if frames == 0 {
            first = f.samples as u64;
        }
        frames += 1;
        samples += f.samples as u64;
        for plane in f.planar {
            for v in plane.iter() {
                if !v.is_finite() {
                    return Err(syom::AacError::decode("non-finite pcm"));
                }
                hash = fnv(hash, &v.to_bits().to_le_bytes());
            }
        }
        Ok(())
    });
    [u64::from(r.is_err()), ch, rate, frames, samples, hash, first]
}

/// Encode interleaved-by-plane f32 PCM (`planes` planes of `n` samples,
/// plane after plane). `mode`: 0 LC 128k ADTS, 1 HE v1 48k ADTS, 2 HE v2
/// 32k ADTS, 3 LC 128k M4A.
pub fn encode_bytes(pcm: &[f32], planes: usize, rate: u32, mode: u32) -> Result<Vec<u8>, syom::AacError> {
    let n = pcm.len() / planes.max(1);
    let views: Vec<&[f32]> = (0..planes).map(|p| &pcm[p * n..(p + 1) * n]).collect();
    let opts = match mode {
        1 => EncodeOptions::low_rate(),
        2 => EncodeOptions::adts().with_he_v2(true).with_bitrate_bps(32_000),
        3 => EncodeOptions::m4a(),
        _ => EncodeOptions::adts(),
    };
    let _ = EncodeContainer::Adts;
    encode_with(&views, rate, &opts)
}

/// Every decoded sample as little-endian f32, frame after frame, plane
/// after plane (the order [`decode_summary`] hashes).
pub fn decode_pcm_bytes(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = decode_streaming(input, &DecodeOptions::unbounded(), |f| {
        for plane in f.planar {
            for v in plane.iter() {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        Ok(())
    });
    out
}

/// # Safety
/// `ptr` / `len` describe initialized bytes inside linear memory.
#[no_mangle]
pub unsafe extern "C" fn syom_decode_pcm(ptr: *const u8, len: usize) -> u32 {
    let out = &mut *std::ptr::addr_of_mut!(OUT);
    *out = decode_pcm_bytes(std::slice::from_raw_parts(ptr, len));
    let r = &mut *std::ptr::addr_of_mut!(RESULT);
    r[8] = out.as_ptr() as u32;
    r[9] = out.len() as u32;
    0
}

fn publish(summary: [u64; 7]) {
    unsafe {
        let r = &mut *std::ptr::addr_of_mut!(RESULT);
        r[0] = summary[0] as u32;
        r[1] = summary[1] as u32;
        r[2] = summary[2] as u32;
        r[3] = summary[3] as u32;
        r[4] = summary[4] as u32;
        r[5] = (summary[4] >> 32) as u32;
        r[6] = summary[5] as u32;
        r[7] = (summary[5] >> 32) as u32;
        r[10] = summary[6] as u32;
    }
}

/// # Safety
/// `ptr` / `len` describe initialized bytes inside linear memory.
#[no_mangle]
pub unsafe extern "C" fn syom_decode(ptr: *const u8, len: usize) -> u32 {
    let s = decode_summary(std::slice::from_raw_parts(ptr, len));
    publish(s);
    s[0] as u32
}

/// # Safety
/// `ptr` points at `planes * n` f32 values.
#[no_mangle]
pub unsafe extern "C" fn syom_encode(ptr: *const f32, planes: usize, n: usize, rate: u32, mode: u32) -> u32 {
    let pcm = std::slice::from_raw_parts(ptr, planes * n);
    let out = &mut *std::ptr::addr_of_mut!(OUT);
    match encode_bytes(pcm, planes, rate, mode) {
        Ok(bytes) => {
            *out = bytes;
            let h = fnv(FNV_SEED, out);
            let r = &mut *std::ptr::addr_of_mut!(RESULT);
            *r = [0, planes as u32, rate, 0, n as u32, 0, h as u32, (h >> 32) as u32, out.as_ptr() as u32, out.len() as u32, 0, 0];
            0
        }
        Err(_) => {
            out.clear();
            (*std::ptr::addr_of_mut!(RESULT))[0] = 1;
            1
        }
    }
}

/// Deterministic test PCM shared by the native and the WASM runs.
pub fn test_pcm(planes: usize, n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(planes * n);
    for p in 0..planes {
        let mut s = 0x0BAD_F00Du32 ^ (p as u32).wrapping_mul(0x9E37_79B9);
        let mut ph = 0u32;
        for _ in 0..n {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ph = ph.wrapping_add(39_370_534 * (p as u32 + 1)); // ≈ 440 Hz·(p+1) at 48 kHz
            let tri = (ph >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            let noise = ((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0;
            out.push(0.4 * tri.abs() - 0.2 + 0.05 * noise);
        }
    }
    out
}

/// Fill a host buffer with [`test_pcm`] so both sides encode the same input.
/// # Safety
/// `ptr` has room for `planes * n` f32 values.
#[no_mangle]
pub unsafe extern "C" fn syom_test_pcm(ptr: *mut f32, planes: usize, n: usize) {
    let v = test_pcm(planes, n);
    std::ptr::copy_nonoverlapping(v.as_ptr(), ptr, v.len());
}
