//! Local: Er Quick Comp — the Project panel's ⚡ button. Footage dropped on it (or selected when it
//! is clicked) becomes a scene's comp structure in one undo step: a precomp per render pass (named
//! after the pass in capitals), the passes stacked in a main comp named
//! `<scene>_EC_<product>_<title>_<MMDDYY>_(1)<maker>` at the Project root, precomps in the root
//! folder `comp` and the footage in `Ft` (folders already there are used).
//!
//! The pass comes from the file name's `_`-separated words (` - ` separates too), in any case:
//! `bgpp`, `bd`, `zd`, `vfx`, `bg`, `all` (checked in that order: a BG file's ZD pass is ZD). A
//! name with a word starting `cryp` (a Cryptomatte EXR) goes to `Ft` but into no comp. A name
//! with none of these is left as it is. The scene is the word after `Sc`; product and title are
//! the two words after `BG` in the BG file's name (the location after them is dropped). What is
//! not found is written as `<Scene>` / `<Project>` for the user to paste over, and the result says
//! so.

use effectcraft_color::Label;
use effectcraft_project::{Comp, ItemId, ItemKind};
use effectcraft_time::{FrameRate, Tick};
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_p};
use crate::{EngineError, Result, Session, cmd};

/// The passes, top layer first in the main comp.
pub const PASSES: [&str; 6] = ["ALL", "VFX", "BGPP", "BG", "ZD", "BD"];
/// Which word picks which pass, in the order they are tried.
const WORDS: [(&str, usize); 6] = [("bgpp", 2), ("bd", 5), ("zd", 4), ("vfx", 1), ("bg", 3), ("all", 0)];
/// The folders, found at the Project root by name (any case) or made there.
pub const COMP_FOLDER: &str = "comp";
pub const FOOTAGE_FOLDER: &str = "Ft";
pub const PROJECT_PLACEHOLDER: &str = "<Project>";
pub const SCENE_PLACEHOLDER: &str = "<Scene>";

/// A file's role in the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Index into [`PASSES`].
    Pass(usize),
    /// A Cryptomatte EXR: goes to `Ft`, into no comp.
    Crypto,
}

/// The `_`/` - ` separated words of an item name, without its extension.
pub fn words(name: &str) -> Vec<String> {
    let stem = match name.rsplit_once('.') {
        Some((a, ext)) if !a.is_empty() && !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => a,
        _ => name,
    };
    stem.replace(" - ", "_").split('_').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

pub fn role(name: &str) -> Option<Role> {
    let w = words(name);
    if w.iter().any(|x| x.to_ascii_lowercase().starts_with("cryp")) {
        return Some(Role::Crypto);
    }
    WORDS.iter().find(|(k, _)| w.iter().any(|x| x.eq_ignore_ascii_case(k))).map(|(_, i)| Role::Pass(*i))
}

/// `08` from `Sc_08_…` (or `Sc08_…`).
pub fn scene(name: &str) -> Option<String> {
    let w = words(name);
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    w.iter().enumerate().find_map(|(i, x)| {
        if x.eq_ignore_ascii_case("sc") {
            w.get(i + 1).filter(|n| digits(n)).cloned()
        } else {
            let (head, rest) = x.split_at_checked(2)?;
            (head.eq_ignore_ascii_case("sc") && digits(rest)).then(|| rest.to_string())
        }
    })
}

/// `Lysol_Carried Away` from `Sc_08_BG_Lysol_Carried Away_Kitchen_111326_…`: the two words after
/// `BG`, when both look like words (not a date, a version `(1)…` or a frame range `[…]`).
pub fn project_name(name: &str) -> Option<String> {
    let w = words(name);
    let i = w.iter().position(|x| x.eq_ignore_ascii_case("bg"))?;
    let ok = |s: &String| !s.chars().all(|c| c.is_ascii_digit()) && !s.starts_with('(') && !s.starts_with('[');
    let product = w.get(i + 1).filter(|s| ok(s))?;
    let title = w.get(i + 2).filter(|s| ok(s))?;
    Some(format!("{product}_{title}"))
}

/// Today in local time as the file names write it: `111326` for 13 Nov 2026.
pub fn today() -> String {
    chrono::Local::now().format("%m%d%y").to_string()
}

fn root_folder(s: &Session, name: &str) -> Option<ItemId> {
    let mut v: Vec<ItemId> =
        s.project.items.values().filter(|i| i.is_folder() && i.parent.is_none() && i.name.eq_ignore_ascii_case(name)).map(|i| i.id).collect();
    v.sort();
    v.first().copied()
}

fn er_quick_comp(s: &mut Session, p: &Value) -> Result<Value> {
    let c = "file.erQuickComp";
    let given: Vec<ItemId> = match p.get("items") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_u64).map(ItemId).collect(),
        _ => s.state.project_selection.clone(),
    };
    let footage: Vec<ItemId> = given.into_iter().filter(|i| matches!(s.project.item(*i).map(|x| &x.kind), Some(ItemKind::Footage(_)))).collect();
    if footage.is_empty() {
        return Err(bad(c, "select footage in the Project panel"));
    }
    let name_of = |s: &Session, i: ItemId| s.project.item(i).map(|x| x.name.clone()).unwrap_or_default();
    let mut passes: Vec<Vec<ItemId>> = vec![vec![]; PASSES.len()];
    let (mut crypto, mut unknown) = (vec![], vec![]);
    for i in &footage {
        match role(&name_of(s, *i)) {
            Some(Role::Pass(k)) => passes[k].push(*i),
            Some(Role::Crypto) => crypto.push(*i),
            None => unknown.push(*i),
        }
    }
    if passes.iter().all(Vec::is_empty) {
        let names: Vec<String> = unknown.iter().map(|i| name_of(s, *i)).collect();
        return Err(bad(c, format!("no file name says ALL, VFX, BGPP, BG, ZD or BD: {}", names.join(", "))));
    }
    let placed: Vec<ItemId> = passes.iter().flatten().copied().collect();
    let scene_found = placed.iter().find_map(|i| scene(&name_of(s, *i)));
    // Product and title from the BG file, else from another pass that names BG (BGPP, ZD).
    let bg_first = passes[3].iter().chain(placed.iter());
    let project_found = bg_first.clone().find_map(|i| project_name(&name_of(s, *i)));
    let date = str_p(p, "date").map(str::to_string).unwrap_or_else(today);
    let maker = str_p(p, "maker").map(str::to_string).unwrap_or_else(|| s.prefs.project.quick_comp_maker.trim().to_string());
    let main_name =
        format!("{}_EC_{}_{}_(1){}", scene_found.as_deref().unwrap_or(SCENE_PLACEHOLDER), project_found.as_deref().unwrap_or(PROJECT_PLACEHOLDER), date, maker);
    let still = if s.prefs.import.still_footage == "seconds" { s.prefs.import.still_seconds } else { 10.0 };
    let still = if still.is_finite() && still > 0.0 { still } else { 10.0 };
    let proj = s.project.clone();
    let duration = |i: ItemId| proj.item(i).and_then(|it| it.duration()).filter(|d| *d > Tick::ZERO).unwrap_or(Tick::from_seconds_f64(still));
    // A comp sized and timed as its footage, as Pre-compose ▸ Leave all attributes makes it.
    let comp_for = |lead: ItemId, dur: Tick| -> Result<Comp> {
        let it = proj.item(lead).ok_or(EngineError::NoComp)?;
        let (w, h) = it.dimensions().unwrap_or((1920, 1080));
        let rate = it.frame_rate().unwrap_or(FrameRate::FPS_29_97);
        let pixel_aspect = match &it.kind {
            ItemKind::Footage(f) => f.pixel_aspect,
            _ => 1.0,
        };
        Ok(Comp { pixel_aspect, ..Comp::new(w.max(1), h.max(1), rate, dur.max(rate.frame_duration())) })
    };
    let made = super::app_more::grouped(s, "Er Quick Comp", |s| {
        let folder = |s: &mut Session, name: &str| -> Result<ItemId> {
            match root_folder(s, name) {
                Some(f) => Ok(f),
                None => s.edit("New Folder", None, |proj, _| Ok(proj.add_item(name, Label::Yellow, None, ItemKind::Folder))),
            }
        };
        let comp_folder = folder(s, COMP_FOLDER)?;
        let ft_folder = folder(s, FOOTAGE_FOLDER)?;
        let mut precomps: Vec<(usize, ItemId, Tick)> = vec![];
        for (k, files) in passes.iter().enumerate() {
            let Some(lead) = files.first() else { continue };
            let dur = files.iter().map(|i| duration(*i)).max().unwrap_or(Tick::from_seconds_f64(still));
            let comp = comp_for(*lead, dur)?;
            let dur = comp.duration;
            let id = s.edit("Er Quick Comp", None, |proj, _| {
                let name = proj.unique_item_name(PASSES[k], |_| false);
                Ok(proj.add_item(&name, Label::Sandstone, Some(comp_folder), ItemKind::Comp(comp.into())))
            })?;
            // Bottom first, each on top: the first file ends up the top layer.
            for f in files.iter().rev() {
                s.execute("layer.addItem", json!({"comp": id.0, "item": f.0, "time": 0, "index": 1, "duration": still}))?;
            }
            precomps.push((k, id, dur));
        }
        let lead = passes.iter().find_map(|f| f.first().copied()).ok_or(EngineError::NoComp)?;
        let dur = precomps.iter().map(|x| x.2).max().unwrap_or(Tick::from_seconds_f64(still));
        let main = comp_for(lead, dur)?;
        let main_id = s.edit("Er Quick Comp", None, |proj, st| {
            let name = proj.unique_item_name(&main_name, |_| false);
            let id = proj.add_item(&name, Label::Sandstone, None, ItemKind::Comp(main.into()));
            for i in placed.iter().chain(crypto.iter()) {
                if let Some(it) = proj.item_mut(*i) {
                    it.parent = Some(ft_folder);
                }
            }
            st.project_selection = vec![id];
            Ok(id)
        })?;
        for (_, id, _) in precomps.iter().rev() {
            s.execute("layer.addItem", json!({"comp": main_id.0, "item": id.0, "time": 0, "index": 1}))?;
        }
        Ok((main_id, precomps, comp_folder, ft_folder))
    })?;
    let (main_id, precomps, comp_folder, ft_folder) = made;
    s.open_comp(main_id);
    s.state.project_selection = vec![main_id];
    let names = |v: &[ItemId]| -> Vec<String> { v.iter().map(|i| name_of(s, *i)).collect() };
    let missing: Vec<&str> = PASSES.iter().enumerate().filter(|(k, _)| passes[*k].is_empty()).map(|(_, n)| *n).collect();
    let pre: Vec<Value> =
        precomps.iter().map(|(k, id, _)| json!({"pass": PASSES[*k], "comp": id.0, "name": name_of(s, *id), "files": names(&passes[*k])})).collect();
    Ok(json!({
        "comp": main_id.0,
        "name": name_of(s, main_id),
        "precomps": pre,
        "missing": missing,
        "crypto": names(&crypto),
        "unknown": names(&unknown),
        "sceneFound": scene_found.is_some(),
        "projectFound": project_found.is_some(),
        "folders": {"comp": comp_folder.0, "footage": ft_folder.0},
    }))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "file.erQuickComp",
        "Er Quick Comp",
        [],
        None,
        "{items?: [id] (default: selected), maker? (default Settings ▸ Project ▸ Er Quick Comp Maker), date? (default today, MMDDYY)} (local: the Project panel's ⚡ — a precomp per pass ALL/VFX/BGPP/BG/ZD/BD from the file names into folder `comp`, footage into `Ft`, main comp `<scene>_EC_<product>_<title>_<date>_(1)<maker>` at the root; one undo step) → {comp, name, precomps, missing, crypto, unknown, sceneFound, projectFound, folders}",
        super::always,
        er_quick_comp
    )]
}
