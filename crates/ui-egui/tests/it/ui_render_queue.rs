//! Render Queue panel input: choosing a Render Settings template from an item's menu (#117).

use effectcraft_engine::Session;
use effectcraft_engine::project::render_queue::RenderQuality;
use effectcraft_ui_egui::EffectcraftApp;
use effectcraft_ui_egui::dock::PanelKind;
use egui::{Event, Modifiers, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

fn click_at(h: &mut Harness<'_, EffectcraftApp>, p: Pos2) {
    h.input_mut().events.push(Event::PointerMoved(p));
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
}

/// The Render Settings menu is taller than the room below the item, so it is moved up to fit
/// the window; its entries must still take the click instead of the menu closing on the press.
#[test]
fn render_settings_template_applies_from_a_menu_moved_to_fit_the_window() {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    s.execute("renderQueue.add", json!({})).unwrap();
    let id = s.project.render_queue[0].id;
    assert_eq!(s.project.render_queue[0].settings.name, "Best Settings");
    let mut app = Some(EffectcraftApp::new(s));
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_eframe(|_| app.take().expect("app"));
    // Twirl the item open, as clicking its twirl does.
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("rq-ui"), (vec![id], None::<u64>)));
    h.state_mut().show_panel(PanelKind::RenderQueue);
    h.run_steps(4);
    let dd = h.state().auto.find(&format!("renderQueue.item.{id}.renderSettings")).expect("Render Settings menu").clone();
    let dd_center = pos2(dd.rect[0] + dd.rect[2] / 2.0, dd.rect[1] + dd.rect[3] / 2.0);
    click_at(&mut h, dd_center);
    let entry = h.query_by_label_contains("Draft Settings").expect("the Draft Settings entry").rect();
    assert!(entry.max.y < dd_center.y, "the menu was moved up to fit the window (entry {entry:?}, menu button at {dd_center:?})");
    click_at(&mut h, entry.center());
    let it = &h.state().session.project.render_queue[0];
    assert_eq!(it.settings.name, "Draft Settings");
    assert_eq!(it.settings.quality, RenderQuality::Draft);
    assert_eq!(it.settings.resolution, 0.5);
    assert!(h.query_by_label_contains("Quality: Draft").is_none(), "the menu closed after the choice");
}

fn click_with(h: &mut Harness<'_, EffectcraftApp>, p: Pos2, modifiers: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(modifiers));
    h.input_mut().events.push(Event::PointerMoved(p));
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers });
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers });
    h.step();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// Issue #192: clicking the Output To file name opens a save dialog (at the current output, for
/// the module's extension) and the chosen file becomes the output, as After Effects' Output
/// Movie To; Alt-click still edits the name template in place.
#[test]
fn clicking_the_output_file_name_opens_the_save_dialog() {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    s.execute("renderQueue.add", json!({})).unwrap();
    let id = s.project.render_queue[0].id;
    let expected_default = s.resolve_output(&s.project.render_queue[0]).unwrap();
    let ext = s.project.render_queue[0].output.format.extension();
    let mut app = EffectcraftApp::new(s);
    let asked: Rc<RefCell<Vec<(String, String)>>> = Rc::default();
    let log = asked.clone();
    let chosen = std::env::temp_dir().join(format!("chosen.{ext}")).to_string_lossy().to_string();
    let answer = chosen.clone();
    app.hooks.pick_save_file = Some(Box::new(move |name: &str, ext: &str| {
        log.borrow_mut().push((name.to_string(), ext.to_string()));
        Some(answer.clone())
    }));
    let mut app = Some(app);
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_eframe(|_| app.take().expect("app"));
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("rq-ui"), (vec![id], None::<u64>)));
    h.state_mut().show_panel(PanelKind::RenderQueue);
    h.run_steps(4);
    let path = h.state().auto.find(&format!("renderQueue.item.{id}.outputPath")).expect("the output file name").clone();
    let at = pos2(path.rect[0] + 20.0, path.rect[1] + path.rect[3] / 2.0);
    click_with(&mut h, at, Modifiers::NONE);
    assert_eq!(*asked.borrow(), [(expected_default, ext.to_string())], "the dialog opened at the current output");
    assert_eq!(h.state().session.project.render_queue[0].output.output, chosen);
    let editing = |h: &Harness<'_, EffectcraftApp>| h.ctx.data(|d| d.get_temp::<(u64, String)>(egui::Id::unique("rq-edit-output")));
    assert_eq!(editing(&h), None, "no inline editor");
    // Alt-click edits the template text instead.
    click_with(&mut h, at, Modifiers::ALT);
    assert_eq!(asked.borrow().len(), 1, "Alt-click does not open the dialog");
    assert_eq!(editing(&h).map(|(item, _)| item), Some(id), "the template editor is open");
}

/// Local: right-click an Output Module template in an item's menu ▸ Set as Default: the open
/// project's Movie Default and the one new projects start from.
#[test]
fn right_click_an_output_module_template_sets_it_as_the_default() {
    use effectcraft_engine::project::render_templates::{TemplateKind, TemplateSlot};
    const MBPS40: &str = "H.264 - Match Render Settings - 40 Mbps";
    // An exporter that writes nothing: the menu lists the templates of its formats.
    let sink: std::sync::Arc<dyn Fn(&str, Vec<u8>) + Send + Sync> = std::sync::Arc::new(|_: &str, _: Vec<u8>| {});
    let mut s = Session { exporter: Some(std::sync::Arc::new(effectcraft_host::FileExporter { sink: Some(sink) })), ..Default::default() };
    s.execute("file.openDemoProject", json!({})).unwrap();
    // Local: a bitrate named on add is kept (Add to Render Queue otherwise makes 15 Mbps 40), so
    // the item does not start on the 40 Mbps template.
    s.execute("renderQueue.add", json!({"bitrate": 15000})).unwrap();
    assert_eq!(s.project.render_queue[0].output.bitrate_kbps, 15_000);
    let id = s.project.render_queue[0].id;
    let mut app = Some(EffectcraftApp::new(s));
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_eframe(|_| app.take().expect("app"));
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("rq-ui"), (vec![id], None::<u64>)));
    h.state_mut().show_panel(PanelKind::RenderQueue);
    h.run_steps(4);
    let dd = h.state().auto.find(&format!("renderQueue.item.{id}.outputModule")).expect("Output Module menu").clone();
    click_at(&mut h, pos2(dd.rect[0] + dd.rect[2] / 2.0, dd.rect[1] + dd.rect[3] / 2.0));
    let entry = h.query_by_label_contains(MBPS40).expect("the 40 Mbps entry").rect();
    let p = entry.center();
    h.input_mut().events.push(Event::PointerMoved(p));
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_ne!(h.state().session.project.render_queue[0].output.name, MBPS40, "a right-click does not choose the template");
    let set = h.query_by_label_contains("Set as Default").expect("the Set as Default entry").rect();
    click_at(&mut h, set.center());
    let movie = |s: &Session| s.project.render_templates.default_name(TemplateKind::OutputModule, TemplateSlot::Movie);
    assert_eq!(movie(&h.state().session), MBPS40);
    assert_eq!(h.state().session.prefs.project.default_output_module, MBPS40);
    assert!(h.query_by_label_contains("Set as Default").is_none(), "the menu closed");
    h.state_mut().session.execute("file.newProject", json!({})).unwrap();
    assert_eq!(movie(&h.state().session), MBPS40, "a new project starts from it");
}
