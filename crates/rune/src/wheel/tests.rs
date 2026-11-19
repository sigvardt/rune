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
