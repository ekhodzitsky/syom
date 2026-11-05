#![allow(dead_code, unused_imports)]
mod error {
 #[derive(Debug)] pub enum GigasttError { AudioTooLong { observed_secs:f64, limit_secs:f64 } }
 impl std::fmt::Display for GigasttError { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"{self:?}")} }
 impl std::error::Error for GigasttError {}
}
mod audio {
 pub const MAX_SAMPLE_RATE:u32=192_000;
 pub fn resolve_budget(secs:Option<f64>,rate:u32)->(usize,f64) { let secs=secs.unwrap_or(f64::INFINITY); ((secs*rate as f64) as usize, secs) }
 pub mod stream { #[derive(Clone,Copy)] pub enum ChannelSelect { Mono, One(usize) } }
 pub mod resample {
  pub struct SampleRate(pub u32);
  pub struct ResampleTo16k;
  impl ResampleTo16k { pub fn new(_:SampleRate,_:Option<usize>)->Self {Self} }
 }
 #[path="/home/ekhodzitsky/Projects/syom/target/task145/classification-guard/aac.rs"] mod aac;
}
