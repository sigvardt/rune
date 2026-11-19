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
    let id = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
    rt.scroll.scrolls[&id].offset
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
        let thrown = rt.throw_velocity();
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
        let id = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("page")).unwrap().id;
        assert!(rt.scroll.scrolls[&id].offset > 150.0, "{seconds} s: the page followed the finger: {}", rt.scroll.scrolls[&id].offset);
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
        let id = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
        assert_eq!(rt.scroll.scrolls[&id].offset, 0.0, "{seconds} s: the list beneath stayed");
        assert!(rt.state.taps.is_empty(), "{seconds} s: nothing pressed: {:?}", rt.state.taps);
    }
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

#[test]
fn a_finger_past_the_slop_picks_one_mode_with_only_its_own_data() {
    let mut rt = rig();
    let list = rt.tree.nodes.iter().find(|n| n.key.as_deref() == Some("list")).unwrap().id;
    rt.touch(1, TouchPhase::Started, 100.0, 200.0, 0.0);
    assert_eq!(rt.pointer.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Pending { scroll: Some(list) }), "pending, it holds the viewport under it");
    rt.touch(1, TouchPhase::Moved, 100.0, 150.0, 0.01);
    assert_eq!(rt.pointer.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Pan { scroll: list, horizontal: false }));
    rt.touch(1, TouchPhase::Ended, 100.0, 150.0, 0.02);
    rt.touch(2, TouchPhase::Started, 20.0, 20.0, 1.0);
    rt.touch(2, TouchPhase::Moved, 60.0, 20.0, 1.01);
    assert_eq!(rt.pointer.touch.finger.as_ref().map(|f| f.mode), Some(Mode::Drag), "on an element that drags it holds no viewport");
}
