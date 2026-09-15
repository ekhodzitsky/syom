//! 4-wide FFT butterflies. Separate mul/add, never FMA, so aarch64 NEON
//! and x86 SSE2 match the scalar rounding (encoder bytes stay exact).

#[cfg(target_arch = "x86_64")]
#[inline]
pub(super) fn sse2_ok() -> bool {
    std::is_x86_feature_detected!("sse2")
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub(super) fn avx_ok() -> bool {
    std::is_x86_feature_detected!("avx")
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn butterfly4(
    re: &mut [f32],
    im: &mut [f32],
    i: usize,
    k: usize,
    half: usize,
    off: usize,
    tw_re: &[f32],
    tw_im: &[f32],
) {
    use std::arch::aarch64::*;
    let j = i + k + half;
    unsafe {
        // SAFETY: NEON is baseline aarch64; caller ensures k+3 < half
        // and i+k+3, j+3 are in-range.
        let wr = vld1q_f32(tw_re.as_ptr().add(off + k));
        let wi = vld1q_f32(tw_im.as_ptr().add(off + k));
        let rj = vld1q_f32(re.as_ptr().add(j));
        let ij = vld1q_f32(im.as_ptr().add(j));
        let tr = vsubq_f32(vmulq_f32(wr, rj), vmulq_f32(wi, ij));
        let ti = vaddq_f32(vmulq_f32(wr, ij), vmulq_f32(wi, rj));
        let ur = vld1q_f32(re.as_ptr().add(i + k));
        let ui = vld1q_f32(im.as_ptr().add(i + k));
        vst1q_f32(re.as_mut_ptr().add(j), vsubq_f32(ur, tr));
        vst1q_f32(im.as_mut_ptr().add(j), vsubq_f32(ui, ti));
        vst1q_f32(re.as_mut_ptr().add(i + k), vaddq_f32(ur, tr));
        vst1q_f32(im.as_mut_ptr().add(i + k), vaddq_f32(ui, ti));
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn butterfly4(
    re: &mut [f32],
    im: &mut [f32],
    i: usize,
    k: usize,
    half: usize,
    off: usize,
    tw_re: &[f32],
    tw_im: &[f32],
) {
    use std::arch::x86_64::*;
    let j = i + k + half;
    unsafe {
        // SAFETY: caller probed SSE2 and ensures k+3 < half and the
        // four-lane loads at i+k and j are in-range. Mul/add/sub only
        // (no `_mm_fmadd_ps`) so lanes match scalar IEEE rounding.
        let wr = _mm_loadu_ps(tw_re.as_ptr().add(off + k));
        let wi = _mm_loadu_ps(tw_im.as_ptr().add(off + k));
        let rj = _mm_loadu_ps(re.as_ptr().add(j));
        let ij = _mm_loadu_ps(im.as_ptr().add(j));
        let tr = _mm_sub_ps(_mm_mul_ps(wr, rj), _mm_mul_ps(wi, ij));
        let ti = _mm_add_ps(_mm_mul_ps(wr, ij), _mm_mul_ps(wi, rj));
        let ur = _mm_loadu_ps(re.as_ptr().add(i + k));
        let ui = _mm_loadu_ps(im.as_ptr().add(i + k));
        _mm_storeu_ps(re.as_mut_ptr().add(j), _mm_sub_ps(ur, tr));
        _mm_storeu_ps(im.as_mut_ptr().add(j), _mm_sub_ps(ui, ti));
        _mm_storeu_ps(re.as_mut_ptr().add(i + k), _mm_add_ps(ur, tr));
        _mm_storeu_ps(im.as_mut_ptr().add(i + k), _mm_add_ps(ui, ti));
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn butterfly8(
    re: &mut [f32],
    im: &mut [f32],
    i: usize,
    k: usize,
    half: usize,
    off: usize,
    tw_re: &[f32],
    tw_im: &[f32],
) {
    use std::arch::x86_64::*;
    let j = i + k + half;
    unsafe {
        // SAFETY: caller probed AVX and ensures k+7 < half. Explicit
        // mul/add/sub, never `_mm256_fmadd_ps`.
        let wr = _mm256_loadu_ps(tw_re.as_ptr().add(off + k));
        let wi = _mm256_loadu_ps(tw_im.as_ptr().add(off + k));
        let rj = _mm256_loadu_ps(re.as_ptr().add(j));
        let ij = _mm256_loadu_ps(im.as_ptr().add(j));
        let tr = _mm256_sub_ps(_mm256_mul_ps(wr, rj), _mm256_mul_ps(wi, ij));
        let ti = _mm256_add_ps(_mm256_mul_ps(wr, ij), _mm256_mul_ps(wi, rj));
        let ur = _mm256_loadu_ps(re.as_ptr().add(i + k));
        let ui = _mm256_loadu_ps(im.as_ptr().add(i + k));
        _mm256_storeu_ps(re.as_mut_ptr().add(j), _mm256_sub_ps(ur, tr));
        _mm256_storeu_ps(im.as_mut_ptr().add(j), _mm256_sub_ps(ui, ti));
        _mm256_storeu_ps(re.as_mut_ptr().add(i + k), _mm256_add_ps(ur, tr));
        _mm256_storeu_ps(im.as_mut_ptr().add(i + k), _mm256_add_ps(ui, ti));
    }
}
