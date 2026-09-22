//! An `OutlineBuilder` that records only *counts* and the verified coordinate extents.
//!
//! A malicious font test must never materialise the (unbounded) expanded path: the
//! whole point of the budgets is that the expansion never happens. So this builder
//! counts callbacks instead of storing them. It also tracks how often each individual
//! callback fired and the min/max coordinates it was asked to draw, which lets a test
//! prove that a rejected glyph produced no callbacks at all and that an accepted glyph
//! never drew outside its own declared coordinate bounds.

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub move_to: u64,
    pub line_to: u64,
    pub quad_to: u64,
    pub curve_to: u64,
    pub close: u64,
}

impl Counts {
    /// Total draw verbs (`quad_to`/`curve_to` carry two/three points but are one
    /// callback). `close` is counted separately because `glyf` emits it while CFF does
    /// not, so bounds that mix outlines compare draw verbs only.
    pub fn draw_verbs(self) -> u64 {
        self.move_to + self.line_to + self.quad_to + self.curve_to
    }

    pub fn total(self) -> u64 {
        self.draw_verbs() + self.close
    }
}

#[derive(Default, Debug)]
pub struct RecordingBuilder {
    pub counts: Counts,
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    seen: bool,
}

impl RecordingBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn track(&mut self, points: &[(f32, f32)]) {
        for &(x, y) in points {
            if !self.seen {
                self.min_x = x;
                self.max_x = x;
                self.min_y = y;
                self.max_y = y;
                self.seen = true;
            } else {
                self.min_x = self.min_x.min(x);
                self.max_x = self.max_x.max(x);
                self.min_y = self.min_y.min(y);
                self.max_y = self.max_y.max(y);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counts.total() == 0
    }

    pub fn seen_coords(&self) -> bool {
        self.seen
    }
}

impl ttf_parser::OutlineBuilder for RecordingBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.counts.move_to += 1;
        self.track(&[(x, y)]);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.counts.line_to += 1;
        self.track(&[(x, y)]);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.counts.quad_to += 1;
        self.track(&[(x1, y1), (x, y)]);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.counts.curve_to += 1;
        self.track(&[(x1, y1), (x2, y2), (x, y)]);
    }

    fn close(&mut self) {
        self.counts.close += 1;
    }
}
