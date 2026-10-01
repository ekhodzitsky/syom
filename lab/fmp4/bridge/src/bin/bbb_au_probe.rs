//! TASK-64 evidence: decode the third-party BBB DASH HE-AAC segment AU-by-AU
//! through syom raw-AU API. Repro of the zero-count FIL rejection finding
//! (REPORT.md §Findings): fails at AU 1 with Malformed(Sbr) on unpatched syom.

fn main() {
    let d = std::fs::read("../out/bbb_cat.mp4").unwrap();
    let init = &d[..633];
    let _ = init;
    // ASC from init (known offset): use walker's: stsd esds asc = 2b118800
    let asc = [0x2b, 0x11, 0x88, 0x00];
    let mut dec = syom::Decoder::from_asc(&asc, syom::DecodeOptions::unbounded()).unwrap();
    // samples resolved by walker: seg1 moof at 701 (633+68), trun data_offset 1220
    // read trun table directly
    let seg = 633usize;
    let trun_body = seg + 132 + 8;
    let count = u32(&d, trun_body + 4) as usize;
    let mut o = trun_body + 8 + 4; // data_offset
    let mut off = seg + 68 + 1220;
    let mut dts = 0u64;
    for i in 0..count {
        let dur = u32(&d, o); let size = u32(&d, o+4) as usize; o += 12;
        let r = dec.decode_au(&d[off..off+size], |_| Ok(()));
        if let Err(e) = r {
            println!("AU {i} (dts {dts}, off {off}, size {size}): {e}");
            break;
        }
        off += size; dts += dur as u64;
    }
    println!("done");
}
fn u32(b:&[u8],o:usize)->u32{u32::from_be_bytes(b[o..o+4].try_into().unwrap())}
