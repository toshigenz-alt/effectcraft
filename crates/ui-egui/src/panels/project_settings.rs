//! Local: Project Panel Settings, opened from the Project panel's ☰ menu. One category for now,
//! Color Display: the colour each kind of Project item's name is drawn in. A kind left at
//! Original keeps the panel's text colour. `app.ui.project_name_colors` holds the choice: `None`
//! is the Default set below, `Some(map)` the user's own (an empty map: every kind Original). It
//! is saved with the workspace layouts (`workspace_state`).

use std::collections::BTreeMap;

use effectcraft_engine::project::{FootageKind, Item, ItemKind};
use egui::{Color32, RichText, Sense, vec2};

use crate::EffectcraftApp;
use crate::i18n::tr;
use crate::widgets;

/// The kinds of Project item, as (key, label), in the order the settings list them.
pub const KINDS: &[(&str, &str)] = &[
    ("comp", "Composition"),
    ("sequence", "Image Sequence"),
    ("video", "Video (MP4, MOV…)"),
    ("audio", "Audio"),
    ("still", "Still Image"),
    ("solid", "Solid"),
    ("folder", "Folder"),
    ("model", "3D Model"),
    ("data", "Data (JSON, CSV)"),
];

const GREEN: [u8; 3] = [0x7b, 0xd8, 0x8f];
const BLUE: [u8; 3] = [0x6c, 0xa8, 0xff];
const YELLOW: [u8; 3] = [0xf2, 0xd3, 0x5b];
const LIGHT_PINK: [u8; 3] = [0xf4, 0xb6, 0xd2];

/// The Default button's colours; the kinds not listed keep their original colour.
pub const DEFAULTS: &[(&str, [u8; 3])] = &[("comp", GREEN), ("sequence", BLUE), ("video", YELLOW), ("audio", LIGHT_PINK)];

/// The colours the list offers (beside Original and the colour picker).
pub const PRESETS: &[(&str, [u8; 3])] = &[
    ("Green", GREEN),
    ("Blue", BLUE),
    ("Yellow", YELLOW),
    ("Light Pink", LIGHT_PINK),
    ("Red", [0xff, 0x6b, 0x6b]),
    ("Orange", [0xff, 0xa0, 0x50]),
    ("Aqua", [0x5f, 0xd7, 0xd7]),
    ("Purple", [0xb3, 0x8c, 0xff]),
    ("Lavender", [0xc8, 0xb4, 0xf0]),
    ("Peach", [0xff, 0xc8, 0xa0]),
    ("Sea Foam", [0x9f, 0xe8, 0xc8]),
    ("Brown", [0xc8, 0x96, 0x6e]),
    ("White", [0xff, 0xff, 0xff]),
    ("Gray", [0x9a, 0x9a, 0x9a]),
];

/// The settings key of `it`'s kind.
pub fn kind_key(it: &Item) -> &'static str {
    match &it.kind {
        ItemKind::Comp(_) => "comp",
        ItemKind::Folder => "folder",
        ItemKind::Solid(_) => "solid",
        ItemKind::Footage(f) => match f.kind {
            FootageKind::Video => "video",
            FootageKind::Still => "still",
            FootageKind::Sequence => "sequence",
            FootageKind::Audio => "audio",
            FootageKind::Model => "model",
            FootageKind::Data => "data",
        },
    }
}

/// The colour set for `key`, `None` for Original.
pub fn color_of(colors: Option<&BTreeMap<String, [u8; 3]>>, key: &str) -> Option<[u8; 3]> {
    match colors {
        Some(map) => map.get(key).copied(),
        None => DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c),
    }
}

/// The colour `it`'s name is drawn in, `None` for the panel's own text colour.
pub fn name_color(app: &EffectcraftApp, it: &Item) -> Option<Color32> {
    color_of(app.ui.project_name_colors.as_ref(), kind_key(it)).map(|c| Color32::from_rgb(c[0], c[1], c[2]))
}

fn unit(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| v as f32 / 255.0)
}

fn bytes(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The Project Panel Settings window, while it is open.
pub fn show(app: &mut EffectcraftApp, ctx: &egui::Context) {
    if !app.ui.project_settings_open {
        return;
    }
    let mut open = true;
    egui::Window::new(tr("Project Panel Settings")).id(egui::Id::unique("project-panel-settings")).open(&mut open).collapsible(false).resizable(false).show(
        ctx,
        |ui| {
            ui.horizontal_top(|ui| {
                // The categories: Color Display only, for now.
                ui.vertical(|ui| {
                    ui.set_width(120.0);
                    let cat = ui.selectable_label(true, tr("Color Display"));
                    app.auto.add("projectSettings.category.colors", cat.rect, "Color Display");
                });
                ui.separator();
                ui.vertical(|ui| color_display(app, ui));
            });
        },
    );
    if !open {
        app.ui.project_settings_open = false;
    }
}

/// Color Display: one row per kind (its name in its colour, the list, the swatch), then Default
/// and Original.
fn color_display(app: &mut EffectcraftApp, ui: &mut egui::Ui) {
    let t = app.tokens;
    ui.label(RichText::new(tr("The colour of each kind of item's name in the Project panel.")).color(t.text_dim));
    ui.add_space(4.0);
    let mut colors: BTreeMap<String, [u8; 3]> =
        KINDS.iter().filter_map(|(k, _)| color_of(app.ui.project_name_colors.as_ref(), k).map(|c| (k.to_string(), c))).collect();
    let mut changed = false;
    egui::Grid::new(egui::Id::unique("project-settings-colors")).num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
        for (key, label) in KINDS {
            let cur = colors.get(*key).copied();
            let shown = cur.map_or(t.text, |c| Color32::from_rgb(c[0], c[1], c[2]));
            ui.label(RichText::new(tr(label)).color(shown));
            let list_text = match cur {
                None => tr("Original").to_string(),
                Some(c) => PRESETS.iter().find(|(_, p)| *p == c).map_or_else(|| widgets::srgb_hex(unit(c)), |(n, _)| tr(n).to_string()),
            };
            let list = craft_ui::ComboBox::from_id_salt(("project-settings-list", *key)).selected_text(list_text).width(130.0).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), tr("Original")).clicked() {
                    colors.remove(*key);
                    changed = true;
                }
                for (name, p) in PRESETS {
                    if ui.selectable_label(cur == Some(*p), RichText::new(tr(name)).color(Color32::from_rgb(p[0], p[1], p[2]))).clicked() {
                        colors.insert(key.to_string(), *p);
                        changed = true;
                    }
                }
            });
            app.auto.add(&format!("projectSettings.{key}.list"), list.response.rect, label);
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 18.0), Sense::hover());
            let pop = egui::Id::unique(("project-settings-pop", *key));
            let mut rgb = unit(cur.unwrap_or([shown.r(), shown.g(), shown.b()]));
            let sw = widgets::swatch(ui, r, [rgb[0], rgb[1], rgb[2], 1.0], egui::Id::unique(("project-settings-swatch", *key)), &t);
            sw.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{} colour", tr(label))));
            app.auto.add(&format!("projectSettings.{key}.swatch"), r, label);
            if sw.clicked() {
                widgets::open_popup(ui, pop);
            }
            if widgets::color_popup(ui, pop, r.left_bottom(), &mut rgb) {
                colors.insert(key.to_string(), bytes(rgb));
                changed = true;
            }
            ui.end_row();
        }
    });
    if changed {
        app.ui.project_name_colors = Some(colors);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let d = ui.button(tr("Default")).on_hover_text(tr("Composition green, image sequence blue, video yellow, audio light pink; the rest original"));
        app.auto.add("projectSettings.default", d.rect, "Default");
        if d.clicked() {
            app.ui.project_name_colors = None;
        }
        let o = ui.button(tr("Original")).on_hover_text(tr("Every kind in the panel's own text colour"));
        app.auto.add("projectSettings.original", o.rect, "Original");
        if o.clicked() {
            app.ui.project_name_colors = Some(BTreeMap::new());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_original_and_custom_colours() {
        // Never set: the Default set; kinds outside it keep the original colour.
        assert_eq!(color_of(None, "comp"), Some(GREEN));
        assert_eq!(color_of(None, "sequence"), Some(BLUE));
        assert_eq!(color_of(None, "video"), Some(YELLOW));
        assert_eq!(color_of(None, "audio"), Some(LIGHT_PINK));
        assert_eq!(color_of(None, "still"), None);
        assert_eq!(color_of(None, "folder"), None);
        // Original: an empty map.
        let original = BTreeMap::new();
        assert!(KINDS.iter().all(|(k, _)| color_of(Some(&original), k).is_none()));
        // A custom colour for one kind.
        let custom: BTreeMap<String, [u8; 3]> = [("still".to_string(), [1, 2, 3])].into();
        assert_eq!(color_of(Some(&custom), "still"), Some([1, 2, 3]));
        assert_eq!(color_of(Some(&custom), "comp"), None, "a user set replaces the defaults");
        // Every default names a listed kind.
        assert!(DEFAULTS.iter().all(|(k, _)| KINDS.iter().any(|(kk, _)| kk == k)));
    }

    #[test]
    fn each_item_kind_has_its_key() {
        use effectcraft_engine::project::{Footage, Project};
        let mut p = Project::default();
        let l = effectcraft_engine::color::Label::None;
        let folder = p.add_item("F", l, None, ItemKind::Folder);
        assert_eq!(kind_key(p.item(folder).unwrap()), "folder");
        for (kind, key) in [
            (FootageKind::Video, "video"),
            (FootageKind::Still, "still"),
            (FootageKind::Sequence, "sequence"),
            (FootageKind::Audio, "audio"),
            (FootageKind::Model, "model"),
            (FootageKind::Data, "data"),
        ] {
            let id = p.add_item(key, l, None, ItemKind::Footage(Footage { kind, ..Default::default() }));
            assert_eq!(kind_key(p.item(id).unwrap()), key);
        }
    }
}
