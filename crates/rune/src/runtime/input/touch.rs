//! Touchscreen and pen input, as a browser handles it: the first finger is the primary
//! button. It presses where it lands, so a tap clicks; dragging an element that drags
//! (`El::on_drag`: a slider, a scroll bar) drags it; any other drag pans the scroll viewport
//! under the finger (`runtime/scroll/touch.rs`), flinging it on with momentum, and cancels
//! the press. A finger held still for [`LONG_PRESS`] is a secondary click there, as Chromium's
//! long press sends `contextmenu`: the press it began lets go without a click and the menu opens
//! under the finger, which then lifts on nothing. Further fingers are ignored while one is down.
use super::*;

/// How far (points) a finger travels before its touch is a drag or a pan, not a tap.
const TOUCH_SLOP: f32 = 8.0;
/// A finger held this long (seconds) before lifting throws nothing.
const FLING_HOLD: f64 = 0.1;
/// The share of the newest move in the speed a lifted finger throws with; the move before it
/// has the rest. UIKit's pan, and so its scroll view, takes its release speed this way: measured
/// in the iOS Simulator against `UIPanGestureRecognizer::velocity` on the same drags, from 235
/// to 2835 points a second, it came within 7 every time, where the newest move alone ran up
/// to 369 under it on a finger slowing as it lifts. The lift's own event is left out, as UIKit
/// leaves it out.
const FLING_NEWEST: f32 = 0.2;
/// A finger held still this long (seconds) opens the menu under it (Chromium's
/// `long_press_time_in_ms`).
const LONG_PRESS: f64 = 0.5;

/// A host's touch phase, as winit reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    /// The system took the touch (a gesture of its own): nothing it pressed is clicked.
    Cancelled,
}

/// What touches are doing: the finger that is down, and the viewport a touch holds or flung.
#[derive(Default)]
pub(super) struct Touch {
    pub(super) scroll: Option<scroll_touch::TouchScroll>,
    finger: Option<Finger>,
}

struct Finger {
    id: u64,
    origin: (f32, f32),
    /// Travel along the pan's axis (vertical until a pan picks horizontal) at the last event,
    /// and that event's time.
    last: f32,
    at: f64,
    /// Points per second along the pan's axis: the speed it throws with (`FLING_NEWEST`).
    velocity: f32,
    /// The newest move's own speed, which the next move weighs against its own.
    segment: Option<f32>,
    mode: Mode,
    /// When (`now_clock`), still pending, it becomes a long press.
    hold_due: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Still within the slop: lifting it here is a tap. `scroll` is the vertical viewport
    /// under the finger, held until the pan picks its axis.
    Pending { scroll: Option<NodeId> },
    /// It drags the element it pressed.
    Drag,
    /// It pans viewport `scroll` along one axis.
    Pan { scroll: NodeId, horizontal: bool },
    /// A long press opened the menu under it: it lifts on nothing.
    Menu,
}

impl Mode {
    /// The viewport the touch holds: the one it pans, or while pending the one under it.
    fn scroll(self) -> Option<NodeId> {
        match self {
            Mode::Pending { scroll } => scroll,
            Mode::Pan { scroll, .. } => Some(scroll),
            Mode::Drag | Mode::Menu => None,
        }
    }

    fn horizontal(self) -> bool { matches!(self, Mode::Pan { horizontal: true, .. }) }
}

impl<S: 'static> Runtime<S> {
    /// One touch event at (x, y) in window points, `time` in seconds on any steady clock. A
    /// host with touches calls this for every one; `id` tells fingers apart.
    pub fn touch(&mut self, id: u64, phase: TouchPhase, x: f32, y: f32, time: f64) {
        match phase {
            TouchPhase::Started => self.touch_down(id, x, y, time),
            TouchPhase::Moved => self.touch_move(id, x, y, time),
            TouchPhase::Ended | TouchPhase::Cancelled => self.touch_up(id, x, y, time, phase == TouchPhase::Cancelled),
        }
    }

    /// Whether a finger is down: a host ignores the mouse events its system makes up from it.
    pub fn touching(&self) -> bool {
        self.touch.finger.is_some()
    }

    fn touch_down(&mut self, id: u64, x: f32, y: f32, time: f64) {
        if self.touch.finger.is_some() { return; }
        let scroll = self.touch_scroll_start(x, y);
        self.pointer_move(x, y);
        self.pointer_down(x, y);
        // `El::touch_drags_at_once`: a drag from the first move, and no long press to cut it.
        if self.press_drags_at_once() {
            self.touch_scroll_end(scroll, 0.0);
            self.touch.finger = Some(Finger { id, origin: (x, y), last: 0.0, at: time, velocity: 0.0, segment: None, mode: Mode::Drag, hold_due: f64::INFINITY });
            return;
        }
        let hold_due = self.now_clock() + LONG_PRESS;
        self.wake_at(hold_due);
        self.touch.finger = Some(Finger { id, origin: (x, y), last: 0.0, at: time, velocity: 0.0, segment: None, mode: Mode::Pending { scroll }, hold_due });
    }

    fn touch_move(&mut self, id: u64, x: f32, y: f32, time: f64) {
        let Some(f) = self.touch.finger.as_ref().filter(|f| f.id == id) else { return };
        let (dx, dy) = (x - f.origin.0, y - f.origin.1);
        if let Mode::Pending { scroll } = f.mode && dx.hypot(dy) > TOUCH_SLOP {
            let origin = f.origin;
            let drags = self.press_drags();
            let (scroll, horizontal) = if drags { (None, false) } else { self.touch_scroll_pan(origin.0, origin.1, dx.abs() > dy.abs(), scroll) };
            self.touch.finger.as_mut().unwrap().mode = match scroll {
                Some(scroll) => Mode::Pan { scroll, horizontal },
                None => Mode::Drag,
            };
            if !drags { self.drop_press_selection(); }
        }
        let f = self.touch.finger.as_mut().unwrap();
        let along = if f.mode.horizontal() { dx } else { dy };
        let dt = time - f.at;
        if dt > 0.0 {
            let segment = (along - f.last) / dt as f32;
            f.velocity = f.segment.map_or(segment, |before| FLING_NEWEST * segment + (1.0 - FLING_NEWEST) * before);
            f.segment = Some(segment);
        }
        (f.last, f.at) = (along, time);
        match f.mode {
            Mode::Pan { scroll, .. } => self.touch_scroll_move(scroll, along),
            Mode::Drag => self.pointer_move(x, y),
            Mode::Pending { .. } | Mode::Menu => {}
        }
    }

    fn touch_up(&mut self, id: u64, x: f32, y: f32, time: f64, cancelled: bool) {
        let Some(f) = self.touch.finger.take_if(|f| f.id == id) else { return };
        let pan = matches!(f.mode, Mode::Pan { .. });
        let velocity = if pan && !cancelled && time - f.at < FLING_HOLD { f.velocity } else { 0.0 };
        self.touch_scroll_end(f.mode.scroll(), velocity);
        if f.mode == Mode::Menu {
            self.pointer_leave();
            return;
        }
        if cancelled {
            self.cancel_pointer();
            return;
        }
        // A pan cancelled the press, so its release clicks nothing.
        let (x, y) = if pan { f.origin } else { (x, y) };
        if f.mode == Mode::Drag { self.pointer_move(x, y); }
        self.pointer_up(x, y);
        // No hover is left under a lifted finger.
        self.pointer_leave();
    }

    /// A finger still within the slop when its long press is due: the press it began lets go
    /// without a click (a drag it started hears a cancelled `Phase::Up`), and a secondary click
    /// lands where it went down.
    pub(super) fn fire_touch_hold(&mut self) {
        let Some(f) = self.touch.finger.as_ref().filter(|f| matches!(f.mode, Mode::Pending { .. }) && self.clock >= f.hold_due) else { return };
        let ((x, y), scroll) = (f.origin, f.mode.scroll());
        self.touch_scroll_end(scroll, 0.0);
        if !self.cancel_drag() { self.drop_press(); }
        self.secondary_down(x, y);
        self.secondary_up(x, y);
        if let Some(f) = self.touch.finger.as_mut() { f.mode = Mode::Menu; }
    }

    /// Whether the held press is on an `El::touch_drags_at_once` element.
    fn press_drags_at_once(&self) -> bool {
        let Some(path) = self.press.path() else { return false };
        self.live_index(path).is_some_and(|i| self.nodes[i].handlers.touch_at_once && self.nodes[i].handlers.on_drag.is_some())
    }

    /// Whether the held press is on an element that drags, so a finger drags it, not the page.
    fn press_drags(&self) -> bool {
        let Some(path) = self.press.path() else { return false };
        self.live_index(path).is_some_and(|i| self.nodes[i].handlers.on_drag.is_some() && !self.drags_out(&self.nodes[i]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScrollSpec, col, el};

    #[derive(Default)]
    struct S { taps: Vec<usize>, drags: Vec<f32> }

    fn view(_: &S, _: &mut Cx<S>) -> El {
        col().size(200.0, 300.0)
            .child(el().key("knob").size(200.0, 40.0).on_drag(|s: &mut S, d: &mut Drag, _| s.drags.push(d.dx)))
            .child(col().key("list").w_full().h(200.0).scroll(ScrollSpec { stick_bottom: false })
                .children((0..30).map(|i| el().key(format!("row{i}")).w_full().h(40.0).on_press(move |s: &mut S| s.taps.push(i)))))
    }

    fn rig() -> Runtime<S> {
        let mut rt = Runtime::new(S::default(), view, (200.0, 300.0), Env::default());
        rt.frame(0.0);
        rt
    }

    fn offset(rt: &Runtime<S>) -> f64 {
        let id = rt.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
        rt.scrolls[&id].offset
    }

    /// A finger from (x, y) moving by `dy` over `steps` events 10 ms apart, then lifted.
    fn swipe(rt: &mut Runtime<S>, (x, y): (f32, f32), dy: f32, steps: usize) {
        rt.touch(1, TouchPhase::Started, x, y, 0.0);
        for i in 1..=steps {
            rt.touch(1, TouchPhase::Moved, x, y + dy * i as f32 / steps as f32, i as f64 * 0.01);
        }
        rt.touch(1, TouchPhase::Ended, x, y + dy, steps as f64 * 0.01 + 0.005);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn a_tap_clicks_what_it_lands_on_and_leaves_no_hover() {
        let mut rt = rig();
        rt.touch(1, TouchPhase::Started, 100.0, 70.0, 0.0);
        rt.touch(1, TouchPhase::Moved, 103.0, 72.0, 0.02);
        rt.touch(1, TouchPhase::Ended, 103.0, 72.0, 0.05);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.taps, [0]);
        assert_eq!(rt.pointer(), None);
        assert!(!rt.touching());
    }

    #[test]
    fn a_drag_on_a_list_scrolls_it_without_clicking_and_flings_on() {
        let mut rt = rig();
        swipe(&mut rt, (100.0, 200.0), -100.0, 5);
        assert!(rt.state.taps.is_empty(), "a pan presses nothing");
        let lifted = offset(&rt);
        assert!(lifted >= 90.0, "the list followed the finger: {lifted}");
        for _ in 0..30 { rt.frame(1.0 / 60.0); }
        assert!(offset(&rt) > lifted + 50.0, "and coasted on: {} after {lifted}", offset(&rt));
    }

    #[test]
    fn a_finger_held_still_before_lifting_throws_nothing() {
        let mut rt = rig();
        rt.touch(1, TouchPhase::Started, 100.0, 200.0, 0.0);
        rt.touch(1, TouchPhase::Moved, 100.0, 150.0, 0.01);
        rt.touch(1, TouchPhase::Ended, 100.0, 150.0, 0.5);
        for _ in 0..30 { rt.frame(1.0 / 60.0); }
        assert_eq!(offset(&rt), 50.0);
    }

    #[test]
    fn a_drag_on_an_element_that_drags_drags_it_not_the_page() {
        let mut rt = rig();
        rt.touch(1, TouchPhase::Started, 20.0, 20.0, 0.0);
        rt.touch(1, TouchPhase::Moved, 60.0, 30.0, 0.01);
        rt.touch(1, TouchPhase::Ended, 60.0, 30.0, 0.02);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.drags.last(), Some(&40.0));
        assert_eq!(offset(&rt), 0.0);
    }

    #[test]
    fn a_second_finger_is_ignored_and_a_cancelled_touch_clicks_nothing() {
        let mut rt = rig();
        rt.touch(1, TouchPhase::Started, 100.0, 70.0, 0.0);
        rt.touch(2, TouchPhase::Started, 100.0, 110.0, 0.0);
        rt.touch(2, TouchPhase::Ended, 100.0, 110.0, 0.01);
        rt.touch(1, TouchPhase::Cancelled, 100.0, 70.0, 0.02);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.taps.is_empty());
        assert!(!rt.touching());
    }

    /// Two drags recorded in the iOS Simulator (the same path, the events' times a little
    /// apart), with the release speed UIKit's pan gave each; the lift is left out, as UIKit
    /// leaves it out.
    #[test]
    fn a_lifted_finger_throws_with_the_speed_uikit_gives_the_same_drag() {
        let path = [538.0, 501.33, 456.0, 408.67, 361.67, 319.0];
        for (times, uikit) in [
            ([0.0, 0.0167, 0.0333, 0.0500, 0.0667, 0.0831], -2775.4),
            ([0.0, 0.0167, 0.0334, 0.0500, 0.0661, 0.0834], -2835.2),
        ] {
            let mut rt = rig();
            rt.touch(1, TouchPhase::Started, 100.0, 230.0, 0.0);
            for (y, t) in path.iter().zip(times).skip(1) {
                rt.touch(1, TouchPhase::Moved, 100.0, 230.0 + (y - path[0]), t);
            }
            let thrown = rt.touch.finger.as_ref().unwrap().velocity;
            assert!((thrown - uikit).abs() < 10.0, "throws {thrown}, UIKit {uikit}");
        }
    }

    /// A list of selectable paragraphs: a finger pans it and marks nothing, however slowly it
    /// moves, as UIKit's text and Chromium's on touch (a drag there scrolls; a selection comes
    /// from a long press).
    #[test]
    fn a_slow_pan_over_selectable_text_scrolls_and_marks_nothing() {
        fn page(_: &(), _: &mut Cx<()>) -> El {
            let words: Vec<String> = (0..300).map(|i| format!("word{i}")).collect();
            col().size(300.0, 300.0).child(col().key("page").w_full().h(300.0).scroll(ScrollSpec { stick_bottom: false })
                .child(crate::text(words.join(" ")).key("notes").w_full().wrap().selectable(true)))
        }
        for (steps, seconds) in [(10, 0.25), (60, 0.9), (150, 2.5)] {
            let mut rt = Runtime::new((), page, (300.0, 300.0), Env::default());
            rt.frame(0.0);
            rt.touch(1, TouchPhase::Started, 60.0, 250.0, 0.0);
            for i in 1..=steps {
                let t = seconds * i as f64 / steps as f64;
                rt.touch(1, TouchPhase::Moved, 60.0, 250.0 - 200.0 * i as f32 / steps as f32, t);
                rt.frame(seconds / steps as f64);
            }
            rt.touch(1, TouchPhase::Ended, 60.0, 50.0, seconds + 0.2);
            for _ in 0..10 { rt.frame(1.0 / 60.0); }
            let id = rt.nodes.iter().find(|n| n.key.as_deref() == Some("page")).unwrap().id;
            assert!(rt.scrolls[&id].offset > 150.0, "{seconds} s: the page followed the finger: {}", rt.scrolls[&id].offset);
            assert_eq!(rt.selected_text(), None, "{seconds} s: a pan marks nothing");
        }
    }

    /// Text with nothing under it to scroll: a finger dragged across it marks nothing either.
    #[test]
    fn a_drag_over_selectable_text_with_nothing_to_scroll_marks_nothing() {
        fn page(_: &(), _: &mut Cx<()>) -> El {
            col().size(300.0, 300.0).child(crate::text("A short note of a few words to drag a finger over").key("note").w_full().wrap().selectable(true))
        }
        let mut rt = Runtime::new((), page, (300.0, 300.0), Env::default());
        rt.frame(0.0);
        rt.touch(1, TouchPhase::Started, 10.0, 8.0, 0.0);
        for i in 1..=30 {
            rt.touch(1, TouchPhase::Moved, 10.0 + 8.0 * i as f32, 8.0, 0.02 * f64::from(i));
            rt.frame(0.02);
        }
        rt.touch(1, TouchPhase::Ended, 250.0, 8.0, 0.7);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.selected_text(), None);
    }

    /// A finger that lands on a button and slides off it, slowly or quickly, presses nothing, as
    /// a UIButton lets go of a touch that leaves it.
    #[test]
    fn a_finger_that_slides_off_a_button_presses_nothing() {
        fn page(_: &S, _: &mut Cx<S>) -> El {
            col().size(300.0, 600.0).justify_end().child(el().key("more").size(80.0, 44.0).on_press(|s: &mut S| s.taps.push(1)))
        }
        // Held still first (a test driver's press-and-drag holds before it moves), or not.
        for hold in [0.0, 0.7] {
            for (steps, seconds) in [(10, 0.25), (36, 0.6), (150, 2.5)] {
                let mut rt = Runtime::new(S::default(), page, (300.0, 600.0), Env::default());
                rt.frame(0.0);
                rt.touch(1, TouchPhase::Started, 40.0, 578.0, 0.0);
                for _ in 0..(hold * 60.0) as usize { rt.frame(1.0 / 60.0); }
                for i in 1..=steps {
                    rt.touch(1, TouchPhase::Moved, 40.0, 578.0 - 300.0 * i as f32 / steps as f32, hold + seconds * i as f64 / steps as f64);
                    rt.frame(seconds / steps as f64);
                }
                rt.touch(1, TouchPhase::Ended, 40.0, 278.0, hold + seconds + 0.02);
                rt.frame(1.0 / 60.0);
                assert!(rt.state.taps.is_empty(), "held {hold} s, {seconds} s: slid off, nothing pressed");
            }
        }
    }

    /// A panel over a list, its button laid over the list's rows: a finger that slides off the
    /// button pans neither the list beneath nor presses the button.
    #[test]
    fn a_finger_on_a_button_over_a_list_beneath_pans_nothing_and_presses_nothing() {
        fn page(_: &S, _: &mut Cx<S>) -> El {
            el().size(300.0, 600.0)
                .child(col().key("list").size(300.0, 600.0).scroll(ScrollSpec { stick_bottom: false })
                    .children((0..40).map(|i| el().key(format!("row{i}")).w_full().h(40.0).on_press(move |s: &mut S| s.taps.push(i)))))
                .child(el().key("more").absolute(10.0, 550.0).size(80.0, 44.0).fill(crate::Color::WHITE).on_press(|s: &mut S| s.taps.push(100)))
        }
        for seconds in [0.25, 0.6, 2.5] {
            let mut rt = Runtime::new(S::default(), page, (300.0, 600.0), Env::default());
            rt.frame(0.0);
            rt.touch(1, TouchPhase::Started, 40.0, 572.0, 0.0);
            for i in 1..=40 {
                rt.touch(1, TouchPhase::Moved, 40.0, 572.0 - 7.5 * i as f32, seconds * f64::from(i) / 40.0);
                rt.frame(seconds / 40.0);
            }
            rt.touch(1, TouchPhase::Ended, 40.0, 272.0, seconds + 0.02);
            for _ in 0..30 { rt.frame(1.0 / 60.0); }
            let id = rt.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
            assert_eq!(rt.scrolls[&id].offset, 0.0, "{seconds} s: the list beneath stayed");
            assert!(rt.state.taps.is_empty(), "{seconds} s: nothing pressed: {:?}", rt.state.taps);
        }
    }

    #[test]
    fn a_finger_past_the_slop_picks_one_mode_with_only_its_own_data() {
        let mut rt = rig();
        let list = rt.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
        rt.touch(1, TouchPhase::Started, 100.0, 200.0, 0.0);
        assert_eq!(rt.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Pending { scroll: Some(list) }), "pending, it holds the viewport under it");
        rt.touch(1, TouchPhase::Moved, 100.0, 150.0, 0.01);
        assert_eq!(rt.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Pan { scroll: list, horizontal: false }));
        rt.touch(1, TouchPhase::Ended, 100.0, 150.0, 0.02);
        rt.touch(2, TouchPhase::Started, 20.0, 20.0, 1.0);
        rt.touch(2, TouchPhase::Moved, 60.0, 20.0, 1.01);
        assert_eq!(rt.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Drag), "on an element that drags it holds no viewport");
    }
}
