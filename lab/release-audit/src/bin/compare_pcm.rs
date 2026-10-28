//! Compare two interleaved f32le decodes at an already verified equal rate/layout.
use serde_json::json;
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let mut reference =
        std::fs::File::open(args.get(1).ok_or("compare_pcm REFERENCE GOT CHANNELS")?)?;
    let mut got = std::fs::File::open(args.get(2).ok_or("missing GOT")?)?;
    let channels: usize = args.get(3).ok_or("missing CHANNELS")?.parse()?;
    if channels == 0 {
        return Err("zero channels".into());
    }
    let (nr, ng) = (reference.metadata()?.len(), got.metadata()?.len());
    if nr != ng || nr % (4 * channels as u64) != 0 {
        println!(
            "{}",
            json!({"comparable":false,"reference_bytes":nr,"got_bytes":ng})
        );
        std::process::exit(1);
    }
    let mut a = vec![0u8; 65536];
    let mut b = vec![0u8; 65536];
    let mut signal = vec![0.0f64; channels];
    let mut noise = signal.clone();
    let mut peak = signal.clone();
    let mut index = 0usize;
    let mut remaining = nr as usize;
    while remaining > 0 {
        let size = remaining.min(a.len());
        reference.read_exact(&mut a[..size])?;
        got.read_exact(&mut b[..size])?;
        for (r, g) in a[..size].chunks_exact(4).zip(b[..size].chunks_exact(4)) {
            let r = f32::from_le_bytes(r.try_into()?) as f64;
            let g = f32::from_le_bytes(g.try_into()?) as f64;
            if !r.is_finite() || !g.is_finite() {
                return Err("non-finite PCM".into());
            }
            let ch = index % channels;
            let delta = r - g;
            signal[ch] += r * r;
            noise[ch] += delta * delta;
            peak[ch] = peak[ch].max(delta.abs());
            index += 1;
        }
        remaining -= size;
    }
    let rows: Vec<_> = (0..channels)
        .map(|ch| {
            json!({"channel":ch,
        "max_abs_s16_lsb":peak[ch]*32768.0,
        "snr_db":if noise[ch]>0.0 {Some(10.0*(signal[ch]/noise[ch]).log10())} else {None},
        "identical":noise[ch]==0.0})
        })
        .collect();
    println!(
        "{}",
        json!({"comparable":true,"samples_per_channel":index/channels,"channels":rows})
    );
    Ok(())
}
