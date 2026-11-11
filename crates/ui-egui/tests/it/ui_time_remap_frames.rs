//! Local: the Timeline shows and takes Time Remap in frames of the comp, while the property keeps
//! storing seconds (the engine, the renderer, AEP and Lottie read seconds).

use effectcraft_engine::Session;
use effectcraft_engine::keyframe::Value;
use effectcraft_engine::project::LayerId;
use effectcraft_ui_egui::EffectcraftApp;
use egui::epaint::Shape;
use egui::{Rect, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn rect_of(h: &Harness<'_, EffectcraftApp>, id: &str) -> Rect {
    let e = h.state().auto.previous.iter().chain(h.state().auto.elements.iter()).find(|e| e.id == id).unwrap_or_else(|| panic!("no {id}")).clone();
    Rect::from_min_size(pos2(e.rect[0], e.rect[1]), vec2(e.rect[2], e.rect[3]))
}

/// The text painted inside `area` in the last frame.
fn texts(h: &Harness<'_, EffectcraftApp>, area: Rect) -> Vec<String> {
    fn walk(s: &Shape, area: Rect, out: &mut Vec<String>) {
        match s {
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, area, out)),
            Shape::Text(t) if area.expand(1.0).contains_rect(t.visual_bounding_rect()) && !t.galley.text().trim().is_empty() => {
                out.push(t.galley.text().trim().to_string());
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for c in &h.output().shapes {
        walk(&c.shape, area, &mut out);
    }
    out
}

fn click(h: &mut Harness<'_, EffectcraftApp>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.step();
    h.step();
}

/// Click a hot number and type a value over it.
fn type_into(h: &mut Harness<'_, EffectcraftApp>, id: &str, text: &str) {
    let p = rect_of(h, id).center();
    click(h, p);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.event(egui::Event::Text(text.into()));
    h.step();
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

/// Drag one pixel per frame, as a slow hand does.
fn slow_drag(h: &mut Harness<'_, EffectcraftApp>, from: egui::Pos2, dx: i32) {
    h.event(egui::Event::PointerMoved(from));
    h.step();
    h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    for k in 1..=dx {
        h.event(egui::Event::PointerMoved(from + vec2(k as f32, 0.0)));
        h.step();
    }
    let to = from + vec2(dx as f32, 0.0);
    h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(3);
}

/// Time Remap's value at the first key (the current time, 0) in seconds.
fn first_key_secs(h: &Harness<'_, EffectcraftApp>, layer: u64) -> f64 {
    let pr = h.state().session.active_comp().unwrap().layer(LayerId(layer)).unwrap().props.get("timeRemap").unwrap().clone();
    match pr.keys[0].value {
        Value::Scalar(v) => v,
        ref v => panic!("not a scalar: {v:?}"),
    }
}

/// Typing a frame stores frame ÷ fps seconds, and the field shows the frame again: whole
/// frames without decimals, a value between two frames with one. 23.976 is the rate where
/// seconds × fps does not come back exact.
#[test]
fn time_remap_is_shown_and_typed_in_frames() {
    for fps in [30.0, 23.976] {
        let mut s = Session::default();
        s.execute("comp.new", json!({"name": "Inner", "width": 64, "height": 64, "frameRate": fps, "duration": 4})).unwrap();
        s.execute("comp.new", json!({"name": "Outer", "width": 64, "height": 64, "frameRate": fps, "duration": 4})).unwrap();
        let l = s.execute("layer.addItem", json!({"item": "Inner"})).unwrap()["layer"].as_u64().unwrap();
        let rate = s.active_comp().unwrap().frame_rate.as_f64();
        let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_eframe(|_| EffectcraftApp::new(s));
        h.run_steps(3);
        effectcraft_ui_egui::menus::invoke(h.state_mut(), &egui::Context::default(), "layer.enableTimeRemap", json!({"layers": [l]})).unwrap();
        h.run_steps(4);
        let uid = h.state().session.active_comp().unwrap().layer(LayerId(l)).unwrap().props.get("timeRemap").unwrap().uid;
        let field = format!("timeline.prop.{uid}.value");
        assert_eq!(texts(&h, rect_of(&h, &field)), vec!["0".to_string()], "{fps} fps: at the source's start");

        for (typed, shown) in [("45", "45"), ("24", "24"), ("12.5", "12.5")] {
            type_into(&mut h, &field, typed);
            let secs = first_key_secs(&h, l);
            let want = typed.parse::<f64>().unwrap() / rate;
            assert!((secs - want).abs() < 1e-9, "{fps} fps: typed {typed} frames, stored {secs} s, want {want} s");
            assert_eq!(texts(&h, rect_of(&h, &field)), vec![shown.to_string()], "{fps} fps: typed {typed}");
        }
    }
}

/// Scrubbing Time Remap moves 1 frame per 10 pixels (user, 2026-11-11): 100 px right from
/// frame 0 lands just short of frame 10 — egui starts a drag only after the pointer has moved a
/// few pixels (measured: 3 px, so 9.7), as for every hot number. 1 frame/px would give ~97.
#[test]
fn time_remap_scrubs_one_frame_per_ten_pixels() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "Inner", "width": 64, "height": 64, "frameRate": 30, "duration": 4})).unwrap();
    s.execute("comp.new", json!({"name": "Outer", "width": 64, "height": 64, "frameRate": 30, "duration": 4})).unwrap();
    let l = s.execute("layer.addItem", json!({"item": "Inner"})).unwrap()["layer"].as_u64().unwrap();
    let mut h = Harness::builder().with_size(vec2(1600.0, 1000.0)).build_eframe(|_| EffectcraftApp::new(s));
    h.run_steps(3);
    effectcraft_ui_egui::menus::invoke(h.state_mut(), &egui::Context::default(), "layer.enableTimeRemap", json!({"layers": [l]})).unwrap();
    h.run_steps(4);
    let uid = h.state().session.active_comp().unwrap().layer(LayerId(l)).unwrap().props.get("timeRemap").unwrap().uid;
    let field = format!("timeline.prop.{uid}.value");
    assert_eq!(first_key_secs(&h, l), 0.0);
    let from = rect_of(&h, &field).center();
    slow_drag(&mut h, from, 100);
    let frames = first_key_secs(&h, l) * 30.0;
    assert!((9.5..=10.0 + 1e-9).contains(&frames), "100 px moved {frames} frames, want ~10");
    assert_eq!(texts(&h, rect_of(&h, &field)), vec![format!("{frames:.1}")]);
}
