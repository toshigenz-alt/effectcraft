//! Local: a name cut short in the Project panel or the Timeline shows in full in a yellow tip
//! by the pointer once the pointer rests on it, as long as no button is held and nothing is
//! being dragged. Names that fit show no tip.

use egui::{Color32, FontId, Rect, Stroke, vec2};

use crate::automation::Registry;

/// How long the pointer rests on a name before its tip shows, in seconds.
pub const DELAY: f32 = 0.4;
const FILL: Color32 = Color32::from_rgb(0xff, 0xf2, 0x9e);
const INK: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
const BORDER: Color32 = Color32::from_gray(0x60);

/// Whether `text` in `font` is wider than `max_w`: drawn cut short.
pub fn is_cut(ui: &egui::Ui, text: &str, font: &FontId, max_w: f32) -> bool {
    ui.painter().layout_no_wrap(text.to_owned(), font.clone(), INK).size().x > max_w
}

/// Shows `text` in a yellow tip by the pointer while it rests on `area`. `key` names the tip in
/// the automation registry (`nameTip.<key>`), so tests and agents can see it.
pub fn show(ui: &egui::Ui, auto: &mut Registry, key: &str, area: Rect, text: &str, font: FontId) {
    let ctx = ui.ctx();
    let Some(pos) = ctx.pointer_hover_pos() else { return };
    if !area.contains(pos) || !ui.rect_contains_pointer(area) || ctx.input(|i| i.pointer.any_down()) || ctx.dragged_id().is_some() {
        return;
    }
    let still = ctx.input(|i| i.pointer.time_since_last_movement());
    if still < DELAY {
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(DELAY - still + 0.02));
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("name-tip")));
    let galley = painter.layout(text.to_owned(), font, INK, 600.0);
    let pad = vec2(6.0, 3.0);
    let mut r = Rect::from_min_size(pos + vec2(12.0, 18.0), galley.size() + pad * 2.0);
    // Kept on the screen: left of the edge, above the pointer at the bottom.
    let screen = ctx.content_rect();
    if r.max.x > screen.max.x {
        r = r.translate(vec2((screen.max.x - r.max.x - 2.0).max(screen.min.x - r.min.x), 0.0));
    }
    if r.max.y > screen.max.y {
        r = r.translate(vec2(0.0, pos.y - 6.0 - r.max.y));
    }
    painter.rect_filled(r, 2.0, FILL);
    painter.rect_stroke(r, 2.0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    painter.galley(r.min + pad, galley, INK);
    auto.add(&format!("nameTip.{key}"), r, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame at `time` with `events`: a 120 pt name cell at (10, 10) holding `text`; returns
    /// the tip the registry saw, if any.
    fn frame(ctx: &egui::Context, time: f64, events: Vec<egui::Event>, text: &str) -> Option<String> {
        let mut auto = Registry::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0));
        let mut out = ctx.run_ui(egui::RawInput { time: Some(time), events, screen_rect: Some(screen), ..Default::default() }, |ui| {
            let cell = Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(120.0, 20.0));
            let font = crate::theme::Tokens::ui(12.0);
            if is_cut(ui, text, &font, cell.width()) {
                show(ui, &mut auto, "cell", cell, text, font);
            }
        });
        out.textures_delta.clear();
        auto.elements.iter().find(|e| e.id == "nameTip.cell").map(|e| e.label.clone())
    }

    #[test]
    fn a_long_name_shows_in_full_after_the_pointer_rests() {
        let long = "A very long footage name that cannot fit the column_v012_final_FINAL.mp4";
        let at = egui::pos2(40.0, 20.0);
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, vec![], long);
        // Just arrived: not yet.
        assert_eq!(frame(&ctx, 1.0, vec![egui::Event::PointerMoved(at)], long), None);
        // Rested past the delay: the whole name.
        assert_eq!(frame(&ctx, 1.0 + f64::from(DELAY) + 0.1, vec![], long).as_deref(), Some(long));
        // A button held (a drag starting): no tip.
        let press = egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() };
        assert_eq!(frame(&ctx, 2.0, vec![press], long), None);
    }

    #[test]
    fn a_name_that_fits_shows_no_tip() {
        let at = egui::pos2(40.0, 20.0);
        let ctx = egui::Context::default();
        frame(&ctx, 0.0, vec![], "Short");
        frame(&ctx, 1.0, vec![egui::Event::PointerMoved(at)], "Short");
        assert_eq!(frame(&ctx, 2.0, vec![], "Short"), None);
    }
}
