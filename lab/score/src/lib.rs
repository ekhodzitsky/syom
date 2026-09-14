//! Isolated encoder/decoder comparison scoring. Not PEAQ-certified.

const SILENCE_RMS: f64 = 1e-6;
const SWAP_RATIO: f64 = 1.25;
const DELAY_DIAG_SAMPLES: i32 = 64;
const TRUNC_RATIO: f64 = 0.5;
const MAX_LAG: i32 = 8192;

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Scored(Score),
    Unscorable { reason: String },
    Diagnostic { kind: DiagKind, detail: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagKind {
    Silent,
    Delayed,
    Truncated,
    ChannelSwap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub delay_samples: i32,
    pub valid_samples: usize,
    pub snr_db: f64,
    pub max_abs: f32,
    pub actual_bps: Option<f64>,
    pub priming_remainder: i64,
}

pub struct Pair<'a> {
    pub rate: u32,
    pub reference: &'a [Vec<f32>],
    pub degraded: &'a [Vec<f32>],
    pub coded_bytes: Option<u64>,
}

pub fn score_pair(p: Pair<'_>) -> Outcome {
    if p.rate == 0 {
        return Outcome::Unscorable {
            reason: "rate=0".into(),
        };
    }
    if p.reference.is_empty() || p.degraded.is_empty() {
        return Outcome::Unscorable {
            reason: "empty planes".into(),
        };
    }
    if p.reference.len() != p.degraded.len() {
        return Outcome::Unscorable {
            reason: format!(
                "channel mismatch ref={} deg={}",
                p.reference.len(),
                p.degraded.len()
            ),
        };
    }
    let n_ref = p.reference[0].len();
    let n_deg = p.degraded[0].len();
    if p.reference.iter().any(|c| c.len() != n_ref) || p.degraded.iter().any(|c| c.len() != n_deg) {
        return Outcome::Unscorable {
            reason: "uneven plane lengths".into(),
        };
    }
    if n_ref == 0 || n_deg == 0 {
        return Outcome::Unscorable {
            reason: "zero samples".into(),
        };
    }
    if rms_planes(p.reference) < SILENCE_RMS || rms_planes(p.degraded) < SILENCE_RMS {
        return Outcome::Diagnostic {
            kind: DiagKind::Silent,
            detail: format!(
                "rms_ref={:.3e} rms_deg={:.3e}",
                rms_planes(p.reference),
                rms_planes(p.degraded)
            ),
        };
    }
    if p.reference.len() == 2 && is_channel_swap(&p.reference, &p.degraded) {
        return Outcome::Diagnostic {
            kind: DiagKind::ChannelSwap,
            detail: "corr(L,R_deg) exceeds corr(L,L_deg)".into(),
        };
    }
    let delay = estimate_delay(&p.reference[0], &p.degraded[0]);
    let (_a, _b, valid) = overlap(&p.reference[0], &p.degraded[0], delay);
    if valid == 0 || (valid as f64) < TRUNC_RATIO * n_ref as f64 {
        return Outcome::Diagnostic {
            kind: DiagKind::Truncated,
            detail: format!("valid={valid} n_ref={n_ref} delay={delay}"),
        };
    }
    if delay.abs() >= DELAY_DIAG_SAMPLES {
        return Outcome::Diagnostic {
            kind: DiagKind::Delayed,
            detail: format!("delay_samples={delay} valid={valid}"),
        };
    }
    let mut max_abs = 0.0f32;
    let mut err = 0.0f64;
    let mut sig = 0.0f64;
    for ch in 0..p.reference.len() {
        let (ra, da, n) = overlap(&p.reference[ch], &p.degraded[ch], delay);
        for i in 0..n {
            let e = ra[i] - da[i];
            max_abs = max_abs.max(e.abs());
            err += (e as f64) * (e as f64);
            sig += (ra[i] as f64) * (ra[i] as f64);
        }
    }
    let snr_db = if err == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (sig / err).log10()
    };
    let dur = valid as f64 / p.rate as f64;
    let actual_bps = p.coded_bytes.map(|b| 8.0 * b as f64 / dur);
    Outcome::Scored(Score {
        delay_samples: delay,
        valid_samples: valid,
        snr_db,
        max_abs,
        actual_bps,
        priming_remainder: n_deg as i64 - n_ref as i64,
    })
}

fn rms_planes(planes: &[Vec<f32>]) -> f64 {
    let mut s = 0.0;
    let mut n = 0usize;
    for p in planes {
        for &x in p {
            s += (x as f64) * (x as f64);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        (s / n as f64).sqrt()
    }
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        s += a[i] as f64 * b[i] as f64;
    }
    s
}

fn is_channel_swap(r: &[Vec<f32>], d: &[Vec<f32>]) -> bool {
    let same = corr(&r[0], &d[0]).abs();
    let cross = corr(&r[0], &d[1]).abs();
    cross > same * SWAP_RATIO && cross > 1e-6
}

fn leading_silence(x: &[f32]) -> usize {
    let thr = 1e-4f32;
    x.iter().take_while(|s| s.abs() < thr).count()
}

fn estimate_delay(r: &[f32], d: &[f32]) -> i32 {
    let pad = leading_silence(d) as i32 - leading_silence(r) as i32;
    if pad.abs() >= DELAY_DIAG_SAMPLES {
        return pad;
    }
    let max_lag = MAX_LAG
        .min((r.len().min(d.len()) / 2) as i32)
        .max(1);
    let min_n = (r.len().min(d.len()) / 2).max(32);
    let mut best_lag = 0i32;
    let mut best = f64::NEG_INFINITY;
    for lag in -max_lag..=max_lag {
        let (a, b, n) = overlap(r, d, lag);
        if n < min_n {
            continue;
        }
        let mut s = 0.0;
        let mut ea = 0.0;
        let mut eb = 0.0;
        for i in 0..n {
            let x = a[i] as f64;
            let y = b[i] as f64;
            s += x * y;
            ea += x * x;
            eb += y * y;
        }
        let denom = (ea * eb).sqrt();
        if denom == 0.0 {
            continue;
        }
        let c = s / denom;
        if c > best + 1e-6 || ((c - best).abs() <= 1e-6 && lag.abs() < best_lag.abs()) {
            best = c;
            best_lag = lag;
        }
    }
    best_lag
}

/// Positive lag: degraded starts later than reference (drop leading deg).
fn overlap<'a>(r: &'a [f32], d: &'a [f32], lag: i32) -> (&'a [f32], &'a [f32], usize) {
    if lag >= 0 {
        let k = lag as usize;
        if k >= d.len() {
            return (&[], &[], 0);
        }
        let n = r.len().min(d.len() - k);
        (&r[..n], &d[k..k + n], n)
    } else {
        let k = (-lag) as usize;
        if k >= r.len() {
            return (&[], &[], 0);
        }
        let n = (r.len() - k).min(d.len());
        (&r[k..k + n], &d[..n], n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, rate: u32, hz: f64, peak: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                (peak as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin())
                    as f32
            })
            .collect()
    }

    #[test]
    fn identical_is_scored_infinite_snr() {
        let r = vec![sine(2048, 48_000, 440.0, 0.5)];
        let o = score_pair(Pair {
            rate: 48_000,
            reference: &r,
            degraded: &r,
            coded_bytes: Some(1000),
        });
        match o {
            Outcome::Scored(s) => {
                assert_eq!(s.delay_samples, 0);
                assert!(s.snr_db.is_infinite());
                assert!(s.actual_bps.unwrap() > 0.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn silence_is_diagnostic_not_infinite_snr() {
        let z = vec![vec![0.0f32; 1024]];
        match score_pair(Pair {
            rate: 48_000,
            reference: &z,
            degraded: &z,
            coded_bytes: None,
        }) {
            Outcome::Diagnostic {
                kind: DiagKind::Silent,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn delay_is_diagnostic_not_a_quality_score() {
        let r = sine(4096, 48_000, 440.0, 0.5);
        let mut d = vec![0.0f32; 512];
        d.extend_from_slice(&r);
        let rp = vec![r];
        let dp = vec![d];
        match score_pair(Pair {
            rate: 48_000,
            reference: &rp,
            degraded: &dp,
            coded_bytes: None,
        }) {
            Outcome::Diagnostic {
                kind: DiagKind::Delayed,
                detail,
            } => assert!(detail.contains("delay_samples=512")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn truncate_is_diagnostic() {
        let r = vec![sine(4096, 48_000, 440.0, 0.5)];
        let d = vec![sine(256, 48_000, 440.0, 0.5)];
        match score_pair(Pair {
            rate: 48_000,
            reference: &r,
            degraded: &d,
            coded_bytes: None,
        }) {
            Outcome::Diagnostic {
                kind: DiagKind::Truncated,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn channel_swap_is_diagnostic() {
        let l = sine(2048, 48_000, 440.0, 0.5);
        let r = sine(2048, 48_000, 660.0, 0.5);
        let rp = vec![l.clone(), r.clone()];
        let dp = vec![r, l];
        match score_pair(Pair {
            rate: 48_000,
            reference: &rp,
            degraded: &dp,
            coded_bytes: None,
        }) {
            Outcome::Diagnostic {
                kind: DiagKind::ChannelSwap,
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn channel_mismatch_is_unscorable() {
        let r = vec![sine(1024, 48_000, 440.0, 0.5)];
        let d = vec![
            sine(1024, 48_000, 440.0, 0.5),
            sine(1024, 48_000, 440.0, 0.5),
        ];
        match score_pair(Pair {
            rate: 48_000,
            reference: &r,
            degraded: &d,
            coded_bytes: None,
        }) {
            Outcome::Unscorable { reason } => assert!(reason.contains("channel")),
            other => panic!("{other:?}"),
        }
    }
}
