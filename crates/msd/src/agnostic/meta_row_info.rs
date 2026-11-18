// Translated to Rust from Etterna's difficulty calculator, MinaCalc:
// https://github.com/etternagame/etterna, src/Etterna/MinaCalc/Agnostic/MetaRowInfo.h
// at commit 2d706147cc4bcf95a8ceb24dc4f4129be207f803 (calculator version
// 527). Changes: see crates/msd/README.md.
//
// Copyright (c) 2016-2023 Etterna <etternadev@gmail.com>.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Row by row hand-agnostic sequencing (the counterpart of metaHandInfo).

use super::ha_sequencing::{
    is_alternating_chord_single, is_alternating_chord_stream, is_jack_at_col,
};
use super::meta_interval_info::MetaItvInfo;
use crate::sequencing_helpers::{MS_INIT, S_INIT, column_count, ms_from};

#[derive(Clone, Debug)]
pub struct MetaRowInfo {
    pub time: f32,
    /// Time from the last row (ms).
    pub ms_now: f32,
    pub count: i32,
    pub last_count: i32,
    pub last_last_count: i32,
    pub notes: u32,
    pub last_notes: u32,
    pub last_last_notes: u32,

    pub alternating_chordstream: bool,
    pub alternating_chord_single: bool,
    pub gluts_maybe: bool,
    pub twas_jack: bool,
}

impl Default for MetaRowInfo {
    fn default() -> Self {
        MetaRowInfo {
            time: S_INIT,
            ms_now: MS_INIT,
            count: 0,
            last_count: 0,
            last_last_count: 0,
            notes: 0,
            last_notes: 0,
            last_last_notes: 0,
            alternating_chordstream: false,
            alternating_chord_single: false,
            gluts_maybe: false,
            twas_jack: false,
        }
    }
}

impl MetaRowInfo {
    pub fn reset(&mut self) {
        *self = MetaRowInfo::default();
    }

    fn set_row_variations(&self, mitvi: &mut MetaItvInfo) {
        if !mitvi.basically_vibro {
            return;
        }
        for k in 0..3 {
            let t = mitvi.row_variations[k];
            if t != 0 {
                if t == self.notes {
                    return;
                }
            } else {
                mitvi.row_variations[k] = self.notes;
                mitvi.num_var += 1;
                if mitvi.row_variations[2] != 0 {
                    mitvi.basically_vibro = false;
                }
                return;
            }
        }
    }

    fn jack_scan(&mut self, mitvi: &mut MetaItvInfo, col_masks: &[u32]) {
        self.twas_jack = false;
        for &id in col_masks {
            if is_jack_at_col(id, self.notes, self.last_notes) {
                mitvi.actual_jacks += 1;
                self.twas_jack = true;
                if self.count > 1 && column_count(self.last_notes) > 1 {
                    mitvi.shared_chord_jacks += 1;
                }
            }
        }
        if self.twas_jack {
            mitvi.actual_jacks_cj += 1;
        }
    }

    fn basic_row_sequencing(
        &mut self,
        last: &MetaRowInfo,
        mitvi: &mut MetaItvInfo,
        col_masks: &[u32],
    ) {
        self.jack_scan(mitvi, col_masks);
        self.set_row_variations(mitvi);

        self.alternating_chordstream =
            is_alternating_chord_stream(self.notes, self.last_notes, last.last_notes);
        if self.alternating_chordstream {
            mitvi.definitely_not_jacks += 1;
        }

        self.alternating_chord_single =
            is_alternating_chord_single(self.count as u32, last.count as u32);
        if self.alternating_chord_single && !self.twas_jack {
            mitvi.seriously_not_js -= 3;
        }

        if last.count == 1 && self.count == 1 {
            mitvi.seriously_not_js = if 0 > mitvi.seriously_not_js {
                0
            } else {
                mitvi.seriously_not_js
            };
            mitvi.seriously_not_js += 1;
            if mitvi.seriously_not_js > 3 {
                mitvi.not_js += mitvi.seriously_not_js;
                mitvi.not_hs += mitvi.seriously_not_js;
            }
        } else if last.count > 1 && self.count > 1 {
            mitvi.not_hs += self.count;
            mitvi.not_js += self.count;
            if (self.notes & self.last_notes) == 0 {
                mitvi.not_hs += 1;
                mitvi.not_js += 1;
            } else {
                self.gluts_maybe = true;
            }
        }

        if (self.notes & self.last_notes) == 0
            && self.count > 1
            && self.last_count > 1
            && (self.last_notes & last.last_notes) == 0
            && self.last_count > 1
        {
            mitvi.dunk_it = true;
        }
    }

    /// `operator()`: this row from the last one.
    pub fn advance(
        &mut self,
        last: &MetaRowInfo,
        mitvi: &mut MetaItvInfo,
        row_time: f32,
        row_count: i32,
        row_notes: u32,
        col_masks: &[u32],
    ) {
        self.time = row_time;
        self.last_last_count = last.last_count;
        self.last_count = last.count;
        self.count = row_count;

        self.last_last_notes = last.last_notes;
        self.last_notes = last.notes;
        self.notes = row_notes;

        self.ms_now = ms_from(self.time, last.time);

        mitvi.itvi.update_tap_counts(self.count);
        self.basic_row_sequencing(last, mitvi, col_masks);
    }
}
