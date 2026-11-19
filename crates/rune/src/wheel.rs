//! [`Wheel`]: one column of a picker's wheel, as UIKit's `UIPickerView` draws it: rows on a
//! drum turned about its axis, the nearer rows taller and brighter, the farther ones squashed
//! towards the drum's edge and faded, and the row in the middle band drawn larger and bright.
//! A finger spins it and it coasts and settles on a row; a tap on a row turns it there.
//!
//! It draws without running the view while it turns: an invisible scroll viewport takes the
//! finger (its coast snapping to rows, `El::scroll_snap`), and a few labels, fewer than the rows
//! that show at once, are moved, squashed, faded and given their words through live values from
//! where the viewport stands (`El::on_scroll_offset`). The view runs once when the wheel comes
//! to rest on a row (`El::on_scroll_rest`), to hear which.
//!
//! The geometry is UIKit's, measured on an iPhone 17 Pro's month and year picker (iOS 26): rows
//! 32 points apart on the finger, 20.6° apart on a drum of radius 88, so the rows beside the
//! middle stand 31.7, 58.5, 77.5 and 87 points from it, each as tall as the cosine of its turn.
//! The band's row is drawn 1.12 times the rows' size.
use std::sync::Arc;

use crate::{Align, Color, El, Live, LiveText, ScrollSpec, TextStyle, Tx, el};

/// Points of finger travel from one row to the next.
pub const PITCH: f32 = 32.0;
/// The wheel's height.
pub const HEIGHT: f32 = 216.0;
/// The middle band's height, where the chosen row is drawn bright.
pub const BAND: f32 = 33.0;
/// The drum's radius.
const RADIUS: f32 = 88.0;
/// The turn from one row to the next, in radians (20.6°).
const STEP: f32 = 0.359_538;
/// The band's row against the others.
const BRIGHT_SCALE: f32 = 1.12;
/// Labels for the rows that can show at once: ±4.4 rows turn the drum a quarter.
const GREY: usize = 11;
/// Labels for the rows that can show in the band: the middle one and one either side.
const BRIGHT: usize = 3;
/// Cycles of a looping wheel's rows: the wheel opens on the middle one, so a finger can spin it
/// thousands of rows either way.
const CYCLES: usize = 201;

/// How a row is drawn with the wheel at a given offset: `y` points from the middle (down),
/// `squash` its height factor, `opacity` its ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Face {
    pub(crate) row: i64,
    pub(crate) y: f32,
    pub(crate) squash: f32,
    pub(crate) opacity: f32,
}

/// Row `row`'s face with the wheel at `offset` (row `r` is in the middle at `r * PITCH`).
pub(crate) fn face(row: i64, offset: f32) -> Face {
    let turn = (row as f32 * PITCH - offset) / PITCH * STEP;
    if turn.abs() >= std::f32::consts::FRAC_PI_2 {
        return Face { row, y: RADIUS.copysign(turn), squash: 0.0, opacity: 0.0 };
    }
    let (sin, cos) = turn.sin_cos();
    // measured: the rows beside the middle at full ink, two away nearly so, then fading with
    // the drum's turn
    Face { row, y: RADIUS * sin, squash: cos, opacity: (0.2 + cos).min(1.0) }
}

/// The rows `n` labels show with the wheel at `offset`: the `n` nearest the middle, each in the
/// label `row mod n`, so a label changes its words only when a row comes in at an edge.
fn faces(offset: f32, n: usize) -> impl Iterator<Item = (usize, Face)> {
    let mid = (offset / PITCH).round() as i64;
    let half = (n / 2) as i64;
    (mid - half..=mid + half).map(move |row| (row.rem_euclid(n as i64) as usize, face(row, offset)))
}

/// The live values that draw one label.
#[derive(Clone)]
struct Slot {
    text: LiveText,
    y: Live<[f32; 2]>,
    squash: Live<f32>,
    opacity: Live<f32>,
}

impl Slot {
    fn new() -> Self {
        Slot { text: LiveText::new(""), y: Live::new([0.0, 0.0]), squash: Live::new(1.0), opacity: Live::new(0.0) }
    }
}

/// What the labels draw, kept apart from the [`Wheel`] so the viewport's handler can hold it.
struct Drum {
    labels: Arc<[Arc<str>]>,
    looped: bool,
    grey: [Slot; GREY],
    bright: [Slot; BRIGHT],
}

impl Drum {
    fn rows(&self) -> i64 {
        (self.labels.len() * if self.looped { CYCLES } else { 1 }) as i64
    }
    /// Turn the labels to the wheel at `offset`.
    fn turn(&self, offset: f32) {
        let (len, rows) = (self.labels.len() as i64, self.rows());
        let words = |row: i64| (0..rows).contains(&row).then(|| self.labels[row.rem_euclid(len.max(1)) as usize].clone());
        for (slots, n, bright) in [(&self.grey[..], GREY, false), (&self.bright[..], BRIGHT, true)] {
            for (i, f) in faces(offset, n) {
                let s = &slots[i];
                match words(f.row) {
                    Some(w) => {
                        s.text.set(w);
                        s.opacity.set(if bright { 1.0 } else { f.opacity });
                    }
                    None => {
                        s.opacity.set(0.0);
                    }
                }
                s.y.set([0.0, f.y]);
                s.squash.set(f.squash);
            }
        }
    }
}

/// How a [`Wheel`] looks: its rows' type and alignment, their ink, the band's row's ink, and
/// the band's fill. The wheel paints that fill across its own width, over the dim rows, and a
/// picker of several wheels paints the same colour behind them for the rounded ends.
#[derive(Clone, Copy, Debug)]
pub struct WheelLook {
    pub font: TextStyle,
    pub align: Align,
    /// Points between the wheel's edge and its words, on the side they align to.
    pub inset: f32,
    /// The same for the band's row, which is drawn larger: UIKit's starts a month's name 3.8 points
    /// before the others'.
    pub bright_inset: f32,
    pub ink: Color,
    pub bright: Color,
    pub band: Color,
}

/// One column of a picker's wheel. Keep it in the app's state, build it once with its rows,
/// and draw it with [`Wheel::view`].
pub struct Wheel {
    key: String,
    drum: Arc<Drum>,
    /// The label chosen.
    at: usize,
    /// The row the wheel stands on (of all the cycles when it loops).
    row: i64,
}

impl Wheel {
    /// A wheel keyed `key` (unique in the window) over `labels`, on label `at`; `looped` wheels
    /// go round, as a month wheel does, the others end at their first and last row.
    pub fn new(key: impl Into<String>, labels: impl IntoIterator<Item = impl Into<Arc<str>>>, at: usize, looped: bool) -> Self {
        let labels: Arc<[Arc<str>]> = labels.into_iter().map(Into::into).collect();
        let at = at.min(labels.len().saturating_sub(1));
        let drum = Arc::new(Drum { labels, looped, grey: std::array::from_fn(|_| Slot::new()), bright: std::array::from_fn(|_| Slot::new()) });
        let mut w = Wheel { key: key.into(), drum, at, row: 0 };
        w.row = w.home(at);
        w.drum.turn(w.row as f32 * PITCH);
        w
    }
    /// The label chosen.
    pub fn at(&self) -> usize {
        self.at
    }
    /// The words of label `i`.
    pub fn label(&self, i: usize) -> Option<&str> {
        self.drum.labels.get(i).map(|s| &**s)
    }
    /// The wheel's key, the scroll viewport's.
    pub fn key(&self) -> &str {
        &self.key
    }
    fn anchor(&self) -> String {
        format!("{}/at", self.key)
    }
    /// Label `at`'s row in the middle cycle.
    fn home(&self, at: usize) -> i64 {
        let len = self.drum.labels.len();
        (if self.drum.looped { len * (CYCLES / 2) } else { 0 } + at) as i64
    }
    /// Label `at`'s row nearest the row the wheel stands on.
    fn nearest(&self, at: usize) -> i64 {
        let len = self.drum.labels.len() as i64;
        if !self.drum.looped || len == 0 {
            return at as i64;
        }
        let base = self.row - self.row.rem_euclid(len) + at as i64;
        [base - len, base, base + len].into_iter().min_by_key(|r| (r - self.row).abs()).unwrap().clamp(0, self.drum.rows() - 1)
    }
    /// Show the wheel on its chosen label in the middle cycle, on the frame that draws it
    /// first: call it in the handler that opens the wheel.
    pub fn open(&mut self, tx: &mut Tx) {
        self.row = self.home(self.at);
        self.drum.turn(self.row as f32 * PITCH);
        tx.scroll_key_to(self.anchor(), (HEIGHT - PITCH) * 0.5);
    }
    /// Choose label `at` and turn the wheel to it.
    pub fn set(&mut self, at: usize, tx: &mut Tx) {
        if at >= self.drum.labels.len() {
            return;
        }
        self.at = at;
        self.row = self.nearest(at);
        tx.scroll_to(self.key.clone(), 0.0, self.row as f32 * PITCH);
    }
    /// Turn `rows` rows on (a looping wheel goes round; the others stop at their ends), as
    /// accessibility's increment and decrement do.
    pub fn step(&mut self, rows: i32, tx: &mut Tx) {
        let len = self.drum.labels.len() as i64;
        if len == 0 {
            return;
        }
        let row = (self.row + i64::from(rows)).clamp(0, self.drum.rows() - 1);
        self.at = row.rem_euclid(len) as usize;
        self.row = row;
        tx.scroll_to(self.key.clone(), 0.0, row as f32 * PITCH);
    }
    /// Where the finger left it: the row nearest `offset`.
    fn rest(&mut self, offset: f32) {
        let len = self.drum.labels.len() as i64;
        if len == 0 {
            return;
        }
        self.row = ((offset / PITCH).round() as i64).clamp(0, self.drum.rows() - 1);
        self.at = self.row.rem_euclid(len) as usize;
    }

    /// The wheel `width` points wide and [`HEIGHT`] tall. `lens` finds it in the app's state;
    /// `picked` hears the label it comes to rest on after a finger turned it, a tap on a row,
    /// or accessibility's increment and decrement.
    pub fn view<S: 'static>(&self, look: &WheelLook, width: f32, lens: fn(&mut S) -> &mut Wheel, picked: fn(&mut S, usize)) -> El {
        let line = look.font.line_px();
        let big = TextStyle { size: look.font.size * BRIGHT_SCALE, ..look.font };
        let big_line = big.line_px();
        let label = |s: &Slot, font: TextStyle, line: f32, ink: Color, top: f32, inset: f32| {
            crate::text("").live_text(&s.text).font(font).color(ink).align(look.align)
                .absolute(if look.align == Align::End { 0.0 } else { inset }, top)
                .size(width - if look.align == Align::Center { 2.0 * inset } else { inset }, line)
                .live_offset(&s.y).live_squash(&s.squash).live_opacity(&s.opacity)
        };
        let mid = HEIGHT * 0.5;
        let grey = self.drum.grey.iter().map(|s| label(s, look.font, line, look.ink, mid - line * 0.5, look.inset));
        // The band's row, in the picker's band colour, over the dim rows: the picker draws the
        // rounded ends behind the wheels, and this fill, the same colour, covers the dim copy
        // of the chosen row that would otherwise show through. Keyed so the picker can place
        // those ends on this rect.
        let band_top = mid - BAND * 0.5;
        let band = el().key(format!("{}/band", self.key)).absolute(0.0, band_top).size(width, BAND).fill(look.band).clip_soft(0.0)
            .children(self.drum.bright.iter().map(|s| label(s, big, big_line, look.bright, BAND * 0.5 - big_line * 0.5, look.bright_inset)));

        // The viewport: row r in the middle at offset r * PITCH, and its rest a multiple of it.
        let rows = self.drum.rows();
        let pad = (HEIGHT - PITCH) * 0.5;
        let mut content = el().w(width).h(pad * 2.0 + rows as f32 * PITCH)
            .child(el().key(self.anchor()).absolute(0.0, pad + self.home(self.at) as f32 * PITCH).size(width, PITCH));
        // A tap on a row turns the wheel to it: the rows the wheel shows at rest, each between
        // the midpoints of its neighbours as drawn.
        let at_rest = self.row as f32 * PITCH;
        for k in -4i64..=4 {
            let row = self.row + k;
            if k == 0 || !(0..rows).contains(&row) {
                continue;
            }
            let (a, b) = (face(row, at_rest).y, |j: i64| face(row + j, at_rest).y);
            let (top, bottom) = ((a + b(-1)) * 0.5, (a + b(1)) * 0.5);
            let (top, bottom) = if k == -4 { (-mid, bottom) } else if k == 4 { (top, mid) } else { (top, bottom) };
            content = content.child(el().absolute(0.0, at_rest + mid + top).size(width, bottom - top)
                .on_press_tx(move |s: &mut S, tx: &mut Tx| {
                    let w = lens(s);
                    w.row = row;
                    w.at = row.rem_euclid(w.drum.labels.len().max(1) as i64) as usize;
                    tx.scroll_to(w.key.clone(), 0.0, row as f32 * PITCH);
                    let at = w.at;
                    picked(s, at);
                }));
        }
        let drum = self.drum.clone();
        let words = self.label(self.at).unwrap_or_default().to_owned();
        let driver = crate::col().key(self.key.clone()).absolute(0.0, 0.0).size(width, HEIGHT)
            .scroll(ScrollSpec { stick_bottom: false }).scroll_snap(PITCH)
            .on_scroll_offset(move |at| drum.turn(at.offset[1]))
            .on_scroll_rest(move |s: &mut S, offset| {
                let w = lens(s);
                w.rest(offset);
                let at = w.at;
                picked(s, at);
            })
            .role("slider").a11y_value(words)
            .on_step(move |s: &mut S, delta, tx| {
                lens(s).step(delta, tx);
                let at = lens(s).at;
                picked(s, at);
            })
            .child(content);
        el().size(width, HEIGHT).clip_soft(0.0).children(grey).child(band).child(driver)
    }
}

#[cfg(test)]
mod tests;
