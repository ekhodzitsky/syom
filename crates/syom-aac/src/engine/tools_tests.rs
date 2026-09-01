//! LC tools: TNS, MS, intensity, PNS, pulse, ICS sequences, SCE/CPE.

use super::bits::BitWriter;
use super::decode::StreamDecoder;
use super::error::Error;
use super::ics::{IcsInfo, WindowSequence, WindowShape};
use super::pns::{self, Lcg};
use super::section::{INTENSITY_HCB, NOISE_HCB, SectionData};
use super::sf::ScaleFactors;
use super::spectrum::{self, PulseData};
use super::stereo::{self, MsInfo, MsMask};
use super::tns::{self, TnsData, TnsFilter, TnsWindow};

fn long_ics(max_sfb: u8) -> IcsInfo {
    IcsInfo {
        window_sequence: WindowSequence::OnlyLong,
        window_shape: WindowShape::Sine,
        max_sfb,
        num_windows: 1,
        num_window_groups: 1,
        window_group_length: [1, 0, 0, 0, 0, 0, 0, 0],
        num_swb: 49,
    }
}

#[test]
fn pulse_adds_amplitude_on_zero_bin() -> Result<(), Error> {
    let mut quant = vec![0i32; 1024];
    let pulse = PulseData {
        start_sfb: 0,
        pulses: vec![(0, 3)],
    };
    spectrum::apply_pulse(&mut quant, 3, &pulse)?;
    // k starts at swb 0 offset 0, plus offset 0 → bin 0; quant[0] was 0 so subtract amp.
    assert_eq!(quant[0], -3);
    Ok(())
}

#[test]
fn ms_dematrix_is_m_plus_s() -> Result<(), Error> {
    let ics = long_ics(1);
    let mut left = vec![0.0f64; 1024];
    let mut right = vec![0.0f64; 1024];
    left[0] = 3.0; // m
    right[0] = 1.0; // s
    let sec = SectionData {
        sfb_cb: vec![vec![1]],
    };
    let ms = MsInfo {
        mask: MsMask::All,
        used: vec![],
    };
    stereo::apply_ms(&mut left, &mut right, &ics, &sec, &sec, &ms, 3)?;
    assert!((left[0] - 4.0).abs() < 1e-12);
    assert!((right[0] - 2.0).abs() < 1e-12);
    Ok(())
}

#[test]
fn intensity_scales_right_from_left() -> Result<(), Error> {
    let ics = long_ics(1);
    let mut left = vec![0.0f64; 1024];
    let mut right = vec![0.0f64; 1024];
    left[0] = 8.0;
    let right_sec = SectionData {
        sfb_cb: vec![vec![INTENSITY_HCB]],
    };
    let sf = ScaleFactors {
        sf: vec![vec![100]],
        is_pos: vec![vec![0]],
        noise_nrg: vec![vec![0]],
    };
    let ms = MsInfo {
        mask: MsMask::Off,
        used: vec![],
    };
    stereo::apply_intensity(&left, &mut right, &ics, &right_sec, &sf, &ms, 3)?;
    assert!((right[0] - 8.0).abs() < 1e-9, "is_pos=0 → scale 1");
    Ok(())
}

#[test]
fn pns_fills_noise_band_energy() -> Result<(), Error> {
    let ics = long_ics(1);
    let mut spec = vec![0.0f64; 1024];
    let sections = SectionData {
        sfb_cb: vec![vec![NOISE_HCB]],
    };
    let sf = ScaleFactors {
        sf: vec![vec![0]],
        is_pos: vec![vec![0]],
        noise_nrg: vec![vec![0]],
    };
    let mut rng = Lcg::new();
    pns::apply(&mut spec, &ics, &sections, &sf, 3, &mut rng, None)?;
    let nrg: f64 = spec.iter().map(|x| x * x).sum();
    // target = 2^(0.25*0) = 1, width=4 → L2 = 1
    assert!((nrg.sqrt() - 1.0).abs() < 1e-9, "PNS L2 {nrg}");
    Ok(())
}

#[test]
fn tns_identity_order_zero_is_noop() -> Result<(), Error> {
    let ics = long_ics(2);
    let mut spec = vec![0.0f64; 1024];
    spec[10] = 1.0;
    let tns = TnsData {
        windows: vec![TnsWindow {
            coef_res: false,
            filters: vec![TnsFilter {
                length: 2,
                order: 0,
                direction: false,
                coef_compress: false,
                coef: vec![],
            }],
        }],
    };
    tns::apply(&mut spec, &tns, &ics, 3)?;
    assert_eq!(spec[10], 1.0);
    Ok(())
}

/// Minimal LC SCE: silence, ONLY_LONG, 48 kHz, max_sfb=1, book 0.
fn silent_sce_payload() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(0, 3); // SCE
    w.write(0, 4); // tag
    w.write(100, 8); // global_gain
    w.write_bit(false); // reserved
    w.write(0, 2); // only_long
    w.write_bit(false); // sine
    w.write(1, 6); // max_sfb
    w.write_bit(false); // no predictor
    w.write(0, 4); // ZERO_HCB
    w.write(1, 5); // sect_len 1
    // no scalefactor (zero band)
    w.write_bit(false); // pulse
    w.write_bit(false); // tns
    w.write_bit(false); // gain
    w.write(7, 3); // END
    w.finish()
}

#[test]
fn sce_silent_frame_is_1024_zeros() -> Result<(), Error> {
    let mut dec = StreamDecoder::new();
    let payload = silent_sce_payload();
    let frame = dec.decode_raw_data_block(2, 3, 48_000, 1, 1, &payload)?;
    assert_eq!(frame.channels, 1);
    assert_eq!(frame.planar[0].len(), 1024);
    assert!(frame.planar[0].iter().all(|&x| x.abs() < 1e-9));
    Ok(())
}

#[test]
fn cpe_common_window_two_channels() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(1, 3); // CPE
    w.write(0, 4);
    w.write_bit(true); // common_window
    // ics
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(1, 6);
    w.write_bit(false);
    w.write(0, 2); // ms_mask off
    for _ in 0..2 {
        w.write(100, 8);
        w.write(0, 4);
        w.write(1, 5);
        w.write_bit(false);
        w.write_bit(false);
        w.write_bit(false);
    }
    w.write(7, 3);
    let payload = w.finish();
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_raw_data_block(2, 3, 48_000, 2, 1, &payload)?;
    assert_eq!(frame.channels, 2);
    assert_eq!(frame.planar[0].len(), 1024);
    assert_eq!(frame.planar[1].len(), 1024);
    Ok(())
}

#[test]
fn long_start_and_stop_synthesize() -> Result<(), Error> {
    use super::filterbank::Filterbank;
    let mut fb = Filterbank::new();
    let spec = vec![0.0f64; 1024];
    for seq in [
        WindowSequence::LongStart,
        WindowSequence::EightShort,
        WindowSequence::LongStop,
    ] {
        let mut ics = long_ics(1);
        ics.window_sequence = seq;
        if seq.is_eight_short() {
            ics.num_windows = 8;
            ics.num_window_groups = 8;
            ics.window_group_length = [1; 8];
            ics.num_swb = 14;
        }
        let pcm = fb.synthesize(&spec, &ics)?;
        assert_eq!(pcm.len(), 1024);
    }
    Ok(())
}
