#![allow(dead_code)]
use std::hint::black_box;
use std::time::Instant;
#[inline(never)]
fn append_old(out: &mut Vec<f32>, planes: &[&[f32]]) {
    if let [left, right] = planes
        && left.len() == right.len()
    {
        out.extend(left.iter().zip(*right).map(|(a, b)| (*a + *b) / 2.0));
        return;
    }
    let Some(first) = planes.first() else { return };
    let start = out.len();
    out.extend_from_slice(first);
    for plane in planes.iter().skip(1) {
        for (sample, value) in out[start..].iter_mut().zip(plane.iter()) {
            *sample += value;
        }
    }
    let count = planes.len() as f32;
    for sample in &mut out[start..] {
        *sample /= count;
    }
}

#[inline(never)]
fn append_new(out: &mut Vec<f32>, planes: &[&[f32]]) {
    if let [mono] = planes {
        out.extend_from_slice(mono);
        return;
    }
    if let [left, right] = planes
        && left.len() == right.len()
    {
        out.extend(left.iter().zip(*right).map(|(a, b)| (*a + *b) / 2.0));
        return;
    }
    let Some(first) = planes.first() else { return };
    let start = out.len();
    out.extend_from_slice(first);
    for plane in planes.iter().skip(1) {
        for (sample, value) in out[start..].iter_mut().zip(plane.iter()) {
            *sample += value;
        }
    }
    let count = planes.len() as f32;
    for sample in &mut out[start..] {
        *sample /= count;
    }
}

fn main() {
 let mut rng=145u32;
 let full:Vec<f32>=(0..193536).map(|_| {rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;f32::from_bits(rng&0xff7fffff)}).collect();
 let planes:&[&[f32]]=&[&full];
 let mut before=Vec::new();let mut after=Vec::new();
 append_old(&mut before,black_box(planes));append_new(&mut after,black_box(planes));
 let differences=before.iter().zip(&after).filter(|(a,b)|a.to_bits()!=b.to_bits()).count();
 assert_eq!(before.len(),full.len());assert_eq!(after.len(),full.len());assert_eq!(differences,0);
 println!("{{\"finite_samples\":{},\"bit_differences\":{},\"samples_per_append\":1024,\"iterations\":20000,\"rounds\":[",full.len(),differences);
 let samples:Vec<f32>=(0..1024).map(|i| (i as f32*0.031).sin()*0.1).collect();
 let storage=[samples.as_slice()];let planes:&[&[f32]]=&storage;
 let mut out=Vec::with_capacity(1024);
 for round in 0..7 {
  for (position,label) in (if round%2==0 {["old","new"]} else {["new","old"]}).iter().enumerate() {
   let f:fn(&mut Vec<f32>,&[&[f32]])=black_box(if *label=="old" {append_old} else {append_new});
   for _ in 0..100 {out.clear();f(&mut out,black_box(planes));black_box(&out);}
   let start=Instant::now();
   for _ in 0..20000 {out.clear();f(&mut out,black_box(planes));black_box(&out);}
   let elapsed=start.elapsed().as_nanos();
   println!("{{\"round\":{},\"label\":\"{}\",\"elapsed_ns\":{},\"ns_per_append\":{}}}{}",round,label,elapsed,elapsed as f64/20000.0,if round==6&&position==1 {""} else {","});
  }
 }
 println!("]}}");
}
