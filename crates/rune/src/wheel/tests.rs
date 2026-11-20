use super::*;
use crate::{Cx, Env, Runtime, TouchPhase};

#[test]
fn the_rows_stand_where_uikits_do() {
    // an iPhone 17 Pro's month and year picker at rest: each row's middle from the band's, and
    // its digits' height, 15.3 points unturned
    for (k, y, tall) in [(1, 31.7, 14.3), (2, 58.5, 11.3), (3, 77.5, 7.0), (4, 87.0, 2.0)] {
        for sign in [1i64, -1] {
            let f = face(k * sign, 0.0);
            assert!((f.y - y * sign as f32).abs() < 1.0, "row {}: {} against {}", k * sign, f.y, y * sign as f32);
            assert!((f.squash * 15.3 - tall).abs() < 0.6, "row {}: {} tall against {tall}", k * sign, f.squash * 15.3);
        }
    }
    assert_eq!(face(0, 0.0), Face { row: 0, y: 0.0, squash: 1.0, opacity: 1.0 });
    assert_eq!(face(5, 0.0).opacity, 0.0, "past a quarter turn nothing shows");
    // half a row on, the two rows either side of the middle stand alike
    let (a, b) = (face(0, PITCH * 0.5), face(1, PITCH * 0.5));
    assert!((a.y + b.y).abs() < 1e-4 && (a.squash - b.squash).abs() < 1e-6);
}

#[test]
fn each_label_keeps_its_row_until_the_row_leaves() {
    let mut last: Vec<Option<i64>> = vec![None; GREY];
    for step in 0..400 {
        let offset = step as f32 * 3.7 - 500.0;
        let now: Vec<(usize, Face)> = faces(offset, GREY).collect();
        let mut seen = [false; GREY];
        for &(i, f) in &now {
            assert!(!seen[i], "one row a label");
            seen[i] = true;
            assert_eq!(f.row.rem_euclid(GREY as i64) as usize, i);
            if let Some(r) = last[i] { assert!(r == f.row || (r - f.row).abs() == GREY as i64, "a label changes rows only by a whole turn of labels"); }
            last[i] = Some(f.row);
        }
        // every row that shows has a label
        let mid = (offset / PITCH).round() as i64;
        for r in mid - 6..=mid + 6 {
            if face(r, offset).opacity > 0.0 { assert!(now.iter().any(|(_, f)| f.row == r), "row {r} at {offset} shows with no label"); }
        }
    }
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

struct St { wheel: Wheel, picks: Vec<usize> }

fn st(looped: bool, at: usize) -> St {
    St { wheel: Wheel::new("months", MONTHS, at, looped), picks: Vec::new() }
}

fn view(s: &St, _: &mut Cx<St>) -> El {
    let look = WheelLook {
        font: TextStyle::new(21.0, 400.0),
        align: Align::Start,
        inset: 20.0,
        bright_inset: 16.0,
        ink: Color::WHITE.alpha(0.5),
        bright: Color::WHITE,
        band: Color::BLACK,
    };
    el().size(300.0, 400.0).child(el().absolute(0.0, 100.0).child(s.wheel.view(&look, 300.0, |s: &mut St| &mut s.wheel, |s: &mut St, at| s.picks.push(at))))
}

fn offset(rt: &Runtime<St>) -> f32 {
    rt.scroll_y_of("months")
}

/// The words the band's labels show and where: (words, y from the band's middle).
fn bright(rt: &Runtime<St>) -> Vec<(String, f32)> {
    rt.state.wheel.drum.bright.iter().filter(|s| s.opacity.get() > 0.0)
        .map(|s| (s.text.get().to_string(), s.y.get()[1])).collect()
}

fn open(looped: bool, at: usize) -> Runtime<St> {
    let mut rt = Runtime::new(st(looped, at), view, (300.0, 400.0), Env::default());
    rt.update(|s, tx| s.wheel.open(tx));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    rt
}

fn pan(rt: &mut Runtime<St>, dy: f32, ms: f64, t0: f64) {
    let steps = (ms / 1000.0 * 60.0) as usize;
    rt.touch(1, TouchPhase::Started, 150.0, 208.0, t0);
    for i in 1..=steps {
        rt.touch(1, TouchPhase::Moved, 150.0, 208.0 + dy * i as f32 / steps as f32, t0 + i as f64 / 60.0);
        rt.frame(1.0 / 60.0);
    }
    rt.touch(1, TouchPhase::Ended, 150.0, 208.0 + dy, t0 + steps as f64 / 60.0);
    for _ in 0..600 { rt.frame(1.0 / 60.0); }
}

#[test]
fn it_opens_on_its_label_in_the_middle_of_the_band() {
    let rt = open(true, 10);
    assert_eq!(offset(&rt), rt.state.wheel.row as f32 * PITCH, "on its row from the first frame");
    let shown = bright(&rt);
    assert!(shown.iter().any(|(w, y)| w == "November" && y.abs() < 1e-3), "{shown:?}");
}

#[test]
fn a_spin_rests_on_a_row_and_tells_it_once() {
    let mut rt = open(true, 10);
    let start = rt.state.wheel.row;
    // three rows and four points, then held still, so the lift throws nothing and it eases onto the third
    let dy = -3.0 * PITCH - 4.0;
    let steps = 24;
    rt.touch(1, TouchPhase::Started, 150.0, 208.0, 1.0);
    for i in 1..=steps {
        rt.touch(1, TouchPhase::Moved, 150.0, 208.0 + dy * i as f32 / steps as f32, 1.0 + i as f64 / 60.0);
        rt.frame(1.0 / 60.0);
    }
    rt.touch(1, TouchPhase::Ended, 150.0, 208.0 + dy, 1.0 + steps as f64 / 60.0 + 0.2);
    for _ in 0..600 { rt.frame(1.0 / 60.0); }
    assert_eq!(offset(&rt) % PITCH, 0.0, "on a row");
    assert_eq!(rt.state.wheel.row, start + 3);
    assert_eq!(rt.state.picks, vec![1], "February, once: {:?}", rt.state.picks);
    assert!(bright(&rt).iter().any(|(w, y)| w == "February" && y.abs() < 1e-3), "{:?}", bright(&rt));
    // a fling the snap aims onto a row, told once more, that row
    pan(&mut rt, 300.0, 100.0, 20.0);
    assert_eq!(offset(&rt) % PITCH, 0.0);
    assert_eq!(rt.state.picks.len(), 2);
    assert_ne!(rt.state.wheel.row, start + 3, "the fling left the row the finger set");
    assert_eq!(rt.state.picks[1], rt.state.wheel.row.rem_euclid(12) as usize);
}

#[test]
fn a_tap_on_a_row_turns_the_wheel_to_it() {
    let mut rt = open(true, 10);
    let start = rt.state.wheel.row;
    // the row two below the middle stands 58.5 points under it
    rt.touch(1, TouchPhase::Started, 150.0, 100.0 + HEIGHT * 0.5 + 58.5, 1.0);
    rt.touch(1, TouchPhase::Ended, 150.0, 100.0 + HEIGHT * 0.5 + 58.5, 1.05);
    for _ in 0..120 { rt.frame(1.0 / 60.0); }
    assert_eq!(rt.state.picks, vec![0], "January");
    assert_eq!(offset(&rt), (start + 2) as f32 * PITCH);
}

/// A year wheel does not loop, so a miss cannot come back round as a month's can. A slow drag
/// of one row, still moving when the finger lifts, rests on the year the finger left it on.
#[test]
fn a_slow_one_row_drag_on_a_year_wheel_rests_on_the_row_under_the_finger() {
    let years: Vec<String> = (1900..2101).map(|y| y.to_string()).collect();
    let drag = |rows: i32| {
        let mut rt = Runtime::new(
            St { wheel: Wheel::new("year", years.clone(), 100, false), picks: Vec::new() },
            view,
            (300.0, 400.0),
            Env::default(),
        );
        rt.update(|s, tx| s.wheel.open(tx));
        rt.frame(0.0);
        rt.frame(1.0 / 60.0);
        let start = rt.state.wheel.row;
        // The finger moves up for a later year. 0.4s, and the last move is the lift, so it flings.
        let dy = -rows as f32 * PITCH;
        let steps = 24;
        rt.touch(1, TouchPhase::Started, 150.0, 208.0, 1.0);
        for i in 1..=steps {
            rt.touch(1, TouchPhase::Moved, 150.0, 208.0 + dy * i as f32 / steps as f32, 1.0 + i as f64 / 60.0);
            rt.frame(1.0 / 60.0);
        }
        rt.touch(1, TouchPhase::Ended, 150.0, 208.0 + dy, 1.0 + steps as f64 / 60.0);
        for _ in 0..600 { rt.frame(1.0 / 60.0); }
        assert_eq!(rt.scroll_y_of("year") % PITCH, 0.0, "on a row");
        assert_eq!(rt.state.wheel.row, start + i64::from(rows), "rows {rows}: the year under the finger");
        assert_eq!(rt.state.wheel.at(), (100 + rows) as usize);
        assert_eq!(rt.state.picks, vec![(100 + rows) as usize]);
    };
    drag(1);
    drag(-1);
}

/// A year wheel, opened on 2000 (label 100 of 1900..=2100).
fn year() -> Runtime<St> {
    let years: Vec<String> = (1900..2101).map(|y| y.to_string()).collect();
    let mut rt = Runtime::new(
        St { wheel: Wheel::new("year", years, 100, false), picks: Vec::new() },
        view,
        (300.0, 400.0),
        Env::default(),
    );
    rt.update(|s, tx| s.wheel.open(tx));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    rt
}

/// A finger drag of `dy` points over `seconds`. `steady` keeps one speed, so the lift throws
/// with it; otherwise the finger eases out and lifts slowly.
fn drag(rt: &mut Runtime<St>, dy: f32, seconds: f64, steady: bool) {
    let steps = (seconds * 60.0).round().max(2.0) as usize;
    let at = |i: usize| {
        let t = i as f64 / steps as f64;
        if steady { t as f32 } else { (t * t * (3.0 - 2.0 * t)) as f32 }
    };
    rt.touch(1, TouchPhase::Started, 150.0, 208.0, 1.0);
    for i in 1..=steps {
        rt.touch(1, TouchPhase::Moved, 150.0, 208.0 + dy * at(i), 1.0 + i as f64 / 60.0);
        rt.frame(1.0 / 60.0);
    }
    rt.touch(1, TouchPhase::Ended, 150.0, 208.0 + dy, 1.0 + steps as f64 / 60.0);
    for _ in 0..600 { rt.frame(1.0 / 60.0); }
}

/// A pan of 96 points over 1.5 seconds is about three rows, and the finger lifts at about 64
/// points a second. Native rests on the third row; projecting that lift's coast lands on the fourth.
#[test]
fn a_medium_pan_rests_on_the_row_under_the_finger() {
    let mut rt = year();
    let start = rt.state.wheel.row;
    drag(&mut rt, -96.0, 1.5, false);
    assert_eq!(rt.scroll_y_of("year") % PITCH, 0.0, "on a row");
    assert_eq!(rt.state.wheel.row, start + 3, "96 points is three rows, and the coast adds none");
    assert_eq!(rt.state.picks, vec![103]);
}

/// A fling of 150 points thrown in a fifth of a second, as the Simulator's fling gesture throws
/// it: the finger moves about five rows and lifts at 750 points a second. UIPickerView's year
/// wheel carries sixteen rows either way (2026 to 2042 and back), and its month wheel, which
/// loops, coasts as far.
#[test]
fn a_fling_carries_as_far_as_native() {
    let flung = |looped: bool, up: bool| {
        let mut rt = if looped { open(true, 10) } else { year() };
        let start = rt.state.wheel.row;
        drag(&mut rt, if up { -150.0 } else { 150.0 }, 0.2, true);
        assert_eq!(rt.scroll_y_of(if looped { "months" } else { "year" }) % PITCH, 0.0, "on a row");
        rt.state.wheel.row - start
    };
    for looped in [false, true] {
        assert_eq!(flung(looped, true), 16, "a fling up carries sixteen rows (looped {looped})");
        assert_eq!(flung(looped, false), -16, "a fling down carries sixteen rows back (looped {looped})");
    }
}

/// One finger's samples, fed to both wheels: where it was, and when. The same list drives the
/// release speed UIKit's pan reports and the speed Rune throws with.
struct Fling {
    /// Travel along the pan, and the event's time. The finger goes down at the first sample and
    /// lifts at the last, and neither of those two counts toward the speed.
    samples: Vec<(f32, f64)>,
}

impl Fling {
    /// What UIKit's pan reports for these samples: a fifth of the newest move and the rest the
    /// move before it. The two recorded drags this was measured on are
    /// [`a_lifted_finger_throws_with_the_speed_uikit_gives_the_same_drag`](crate::runtime::input::touch::tests).
    fn uikit(&self) -> f32 {
        let moved: Vec<(f32, f64)> = self.samples.iter().copied().skip(1).take(self.samples.len() - 2).collect();
        let seg = |i: usize| (moved[i].0 - moved[i - 1].0) / (moved[i].1 - moved[i - 1].1) as f32;
        let n = moved.len();
        0.2 * seg(n - 1) + 0.8 * seg(n - 2)
    }

    fn play(&self, rt: &mut Runtime<St>) {
        let (y0, t0) = self.samples[0];
        rt.touch(1, TouchPhase::Started, 150.0, 208.0, t0);
        for (at, time) in self.samples.iter().skip(1) {
            rt.touch(1, TouchPhase::Moved, 150.0, 208.0 + (at - y0), *time);
            rt.frame(1.0 / 60.0);
        }
        let (end, tend) = self.samples[self.samples.len() - 1];
        rt.touch(1, TouchPhase::Ended, 150.0, 208.0 + (end - y0), tend);
        for _ in 0..600 { rt.frame(1.0 / 60.0); }
    }
}

/// The rows a coast of `speed` points a second settles on, past the rows the finger itself
/// travelled: UIScrollView's own deceleration (`-1000 * ln(0.998)`), then the nearest row.
fn coast_rows(speed: f32, travelled: f32) -> i64 {
    let travel = f64::from(-speed) / 2.002;
    let coast = if travel.abs() > 4.0 * f64::from(PITCH) { travel } else { 0.0 };
    ((f64::from(travelled) + coast) / f64::from(PITCH)).round() as i64
}

/// agent-device's `gesture fling` (0.21.31) as its plan describes it: the finger travels the
/// distance in a tenth of a second, the pointer is held for the plan's own tenth, and it lifts.
/// Sampled at 60 Hz, which is the stream both wheels are asked with below.
fn agent_fling(dy: f32) -> Fling {
    let mut samples = vec![(0.0, 1.0)];
    let frames = (0.2 * 60.0) as usize;
    for i in 1..frames {
        let t = i as f64 / 60.0;
        samples.push((dy * (t / 0.1).min(1.0) as f32, 1.0 + t));
    }
    samples.push((dy, 1.0 + frames as f64 / 60.0));
    Fling { samples }
}

/// One sample stream, fed once to UIKit's estimate and once to Rune: the two agree on it. The
/// finger travels at one speed and then holds still, so the last two moves are both still and both
/// estimates throw nothing. The wheel then rests on the row under the finger: 100 points is three
/// rows and 150 is five, on a wheel that loops and on one that stops. #375 measured ten and sixteen
/// on this gesture, so the samples XCTest delivered there are not this stream.
#[test]
fn the_same_fling_lands_where_both_estimates_land() {
    for dy in [-100.0_f32, -150.0] {
        let fling = agent_fling(dy);
        let expect = coast_rows(fling.uikit(), -dy);
        for looped in [false, true] {
            let mut rt = if looped { open(true, 10) } else { year() };
            let start = rt.state.wheel.row;
            fling.play(&mut rt);
            assert_eq!(rt.scroll_y_of(if looped { "months" } else { "year" }) % PITCH, 0.0, "on a row");
            assert_eq!(rt.state.wheel.row - start, expect, "{dy} points, UIKit's estimate and Rune's (looped {looped})");
        }
    }
}

/// The band's row is the wheel's own middle, [`BAND`] tall, filled with the band's colour over
/// the dim rows.
#[test]
fn the_bands_row_sits_in_the_middle_of_the_wheel() {
    let rt = open(true, 10);
    let ([x, y, w, h], _) = rt.presented("months/band").expect("the band's row");
    assert_eq!((x, w, h), (0.0, 300.0, BAND));
    assert!((y - (100.0 + (HEIGHT - BAND) * 0.5)).abs() < 1e-3, "the band's top: {y}");
}

#[test]
fn a_wheel_that_does_not_loop_stops_at_its_ends() {
    let mut rt = open(false, 1);
    pan(&mut rt, 600.0, 120.0, 1.0);
    assert_eq!(offset(&rt), 0.0);
    assert_eq!(rt.state.picks.last(), Some(&0));
    pan(&mut rt, -2000.0, 120.0, 20.0);
    assert_eq!(offset(&rt), 11.0 * PITCH);
    assert_eq!(rt.state.picks.last(), Some(&11));
    // above the first row nothing shows
    assert!(rt.state.wheel.drum.grey.iter().filter(|s| s.text.get().as_ref() == "January").count() <= 1);
}
