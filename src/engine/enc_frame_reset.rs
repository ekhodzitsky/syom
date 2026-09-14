//! In-place [`super::LcEncoder`] reset: keep windows/psy, drop signal.

use super::LcEncoder;
use crate::engine::enc_group::Grouping;
use crate::engine::enc_ms::MsBands;
use crate::engine::enc_psy::AttackDetector;
use crate::engine::enc_quant::{MAX_BANDS, MAX_FLAT_SHORT, QuantChannel, QuantShort};
use crate::engine::enc_tns::EncTns;
use crate::engine::ics::WindowSequence;
use crate::engine::swb::LONG_WINDOW_LEN;

impl LcEncoder {
    /// Zero overlap, rate credit, detectors, and lookahead. Prepared KBD
    /// windows, psy spreading matrices, and quant boxes stay allocated.
    pub(crate) fn reset(&mut self) {
        self.prev = [[0.0; LONG_WINDOW_LEN]; 2];
        let n_bands = self.chans[0].n_bands;
        self.chans[0] = QuantChannel::new(n_bands);
        self.chans[1] = QuantChannel::new(n_bands);
        self.books = [[0; MAX_BANDS]; 2];
        self.gains = [100; 2];
        self.target_q = [[0.0; MAX_BANDS]; 2];
        self.credit = 0;
        self.pad_debt = 0;
        self.ms = MsBands::off();
        self.tns = [EncTns::off(), EncTns::off()];
        self.prev_seq = WindowSequence::OnlyLong;
        self.seq = WindowSequence::OnlyLong;
        self.detectors = [AttackDetector::new(), AttackDetector::new()];
        self.is_band = [false; MAX_BANDS];
        self.grouping = Grouping::ungrouped();
        self.held = None;
        let n_sfb = self.chans_s[0].n_sfb;
        self.chans_s[0] = QuantShort::new(n_sfb);
        self.chans_s[1] = QuantShort::new(n_sfb);
        self.books_s = [[0; MAX_FLAT_SHORT]; 2];
        self.target_q_s = [[0.0; MAX_FLAT_SHORT]; 2];
    }
}
