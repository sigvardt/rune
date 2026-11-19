//! `El::live_squash`: a text's glyphs drawn shorter about the text's middle, as wide as before,
//! with no view run, under a `live_offset` too.
use super::*;
use crate::{Cx, El, Env, Live, Runtime, el};

type State = (Live<f32>, Live<[f32; 2]>);

fn view(s: &State, _: &mut Cx<State>) -> El {
    el().size(200.0, 120.0).child(
        el().absolute(20.0, 30.0).size(160.0, 40.0).live_offset(&s.1).child(
            crate::text("HHHH").size(160.0, 40.0).font(crate::TextStyle::new(28.0, 400.0)).color(crate::Color::WHITE).live_squash(&s.0),
        ),
    )
}

/// The glyphs' bounds `[x0, y0, x1, y1]`, in device pixels.
fn ink(rt: &Runtime<State>) -> [f32; 4] {
    let g: Vec<_> = rt.instances.iter().filter(|q| q.params[1] == KIND_GLYPH).collect();
    assert_eq!(g.len(), 4, "four glyphs");
    g.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, q| {
        [b[0].min(q.rect[0]), b[1].min(q.rect[1]), b[2].max(q.rect[0] + q.rect[2]), b[3].max(q.rect[1] + q.rect[3])]
    })
}

#[test]
fn a_squash_draws_the_glyphs_shorter_about_the_middle_of_the_text_as_wide_and_runs_no_view() {
    for scale in [1.0f32, 3.0] {
        let (k, o) = (Live::new(1.0f32), Live::new([0.0f32, 0.0]));
        let mut rt = Runtime::new((k.clone(), o.clone()), view, (200.0, 120.0), Env::new(true, scale));
        rt.frame(0.0);
        let rest = ink(&rt);
        let views = rt.commit.commits;
        // the text's middle: its box is 30..70 points down
        let mid = 50.0 * scale;
        for (squash, dy) in [(0.5f32, 0.0f32), (0.5, 24.0), (0.25, -10.0), (1.0, 24.0)] {
            k.set(squash);
            o.set([0.0, dy]);
            rt.frame(0.1);
            let now = ink(&rt);
            let at = |y: f32| mid + dy * scale + (y - mid) * squash;
            let near = |a: f32, b: f32| (a - b).abs() < 0.51;
            assert!(near(now[0], rest[0]) && near(now[2], rest[2]), "scale {scale} squash {squash}: as wide, {rest:?} then {now:?}");
            assert!(near(now[1], at(rest[1])) && near(now[3], at(rest[3])), "scale {scale} squash {squash} dy {dy}: {rest:?} then {now:?}");
        }
        assert_eq!(rt.commit.commits, views, "a squash runs no view");
    }
}
