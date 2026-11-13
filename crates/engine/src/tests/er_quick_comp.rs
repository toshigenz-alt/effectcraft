//! Local: Er Quick Comp — the Project panel's ⚡: a scene's footage becomes a precomp per pass,
//! stacked in a main comp named from the file names, precomps in `comp`, footage in `Ft`.

use serde_json::{Value, json};

use crate::Session;
use crate::cmd::er_quick_comp::{Role, project_name, role, scene};

/// The user's screenshot of 2026-11-13 (Project panel, scene 08).
const ALL: &str = "Sc_08_All_111326_(1)AMP_RGB_[0001-0018].png";
const BG: &str = "Sc_08_BG_Lysol_Carried Away_Kitchen+LivingroomV3_111326_(1)PremP_RGB_[0001-0018].png";
const ZD: &str = "Sc_08_BG_Lysol_Carried Away_Kitchen+LivingroomV3_111326_(1)PremP_ZD_[0001-0018].png";
const CRYP: &str = "Sc_08_VFX_111326_(1)Er_CrypM_[0001-0018].exr";
const VFX: &str = "Sc_08_VFX_111326_(1)Er_RGB_[0001-0018].png";

fn footage(s: &mut Session, name: &str, w: u32, h: u32) -> u64 {
    s.execute("file.importPlaceholder", json!({"name": name, "width": w, "height": h, "frameRate": 30, "duration": 0.6})).unwrap()["item"].as_u64().unwrap()
}

fn item(s: &Session, id: u64) -> &effectcraft_project::Item {
    s.project.item(effectcraft_project::ItemId(id)).unwrap()
}

fn layer_names(s: &Session, comp: u64) -> Vec<String> {
    s.project.comp(effectcraft_project::ItemId(comp)).unwrap().layers.iter().map(|l| l.name.clone()).collect()
}

fn parent_name(s: &Session, id: u64) -> Option<String> {
    item(s, id).parent.map(|p| s.project.item(p).unwrap().name.clone())
}

#[test]
fn file_names_give_the_pass_the_scene_and_the_project() {
    assert_eq!(role(ALL), Some(Role::Pass(0)));
    assert_eq!(role(VFX), Some(Role::Pass(1)));
    assert_eq!(role(BG), Some(Role::Pass(3)));
    assert_eq!(role(ZD), Some(Role::Pass(4)), "a BG file's ZD pass is ZD");
    assert_eq!(role(CRYP), Some(Role::Crypto));
    assert_eq!(role("Sc_08_BG_Lysol_Carried Away_Kitchen_111326_(1)PremP_BGPP_[0001-0018].png"), Some(Role::Pass(2)));
    assert_eq!(role("Sc_08_bd_111326.png"), Some(Role::Pass(5)));
    assert_eq!(role("Sc_08_Smoke_111326_(1)Er_RGB.png"), None);
    assert_eq!(role("allover.mov"), None, "whole words only");
    assert_eq!(scene(ALL).as_deref(), Some("08"));
    assert_eq!(scene("SC12_All.png").as_deref(), Some("12"));
    assert_eq!(scene("All_111326.png"), None);
    assert_eq!(project_name(BG).as_deref(), Some("Lysol_Carried Away"));
    assert_eq!(project_name("Sc_08 - BG - Lysol - Carried Away - Kitchen_111326.png").as_deref(), Some("Lysol_Carried Away"), "` - ` separates too");
    assert_eq!(project_name("Sc_08_BG_Lysol_111326_(1)PremP_RGB.png"), None, "a date is not a title");
    assert_eq!(project_name(VFX), None);
}

#[test]
fn the_screenshot_scene_becomes_its_comps_and_folders_in_one_undo_step() {
    let mut s = Session::default();
    // A `comp` folder already there is used (any case), not made again.
    let existing = s.execute("project.newFolder", json!({"name": "Comp"})).unwrap()["item"].as_u64().unwrap();
    let ids: Vec<u64> = [ALL, BG, ZD, CRYP, VFX].iter().map(|n| footage(&mut s, n, 1920, 1080)).collect();
    let other = footage(&mut s, "reference.png", 100, 100);
    let before = s.project.items.len();
    let mut items = ids.clone();
    items.push(other);
    let r = s.execute("file.erQuickComp", json!({"items": items, "date": "111326"})).unwrap();

    assert_eq!(r["name"], "08_EC_Lysol_Carried Away_111326_(1)Er");
    let main = r["comp"].as_u64().unwrap();
    assert_eq!(item(&s, main).parent, None, "the main comp is at the root");
    assert_eq!(layer_names(&s, main), ["ALL", "VFX", "BG", "ZD"]);
    let c = s.project.comp(effectcraft_project::ItemId(main)).unwrap();
    assert_eq!((c.width, c.height), (1920, 1080));
    for p in r["precomps"].as_array().unwrap() {
        let id = p["comp"].as_u64().unwrap();
        assert_eq!(item(&s, id).parent.map(|x| x.0), Some(existing), "{p}");
        assert_eq!(layer_names(&s, id).len(), 1, "{p}");
    }
    assert_eq!(layer_names(&s, r["precomps"][1]["comp"].as_u64().unwrap()), [VFX], "the CrypM file goes into no comp");
    for id in &ids {
        assert_eq!(parent_name(&s, *id).as_deref(), Some("Ft"), "{}", item(&s, *id).name);
    }
    assert_eq!(item(&s, other).parent, None, "an unknown file is left as it is");
    assert_eq!(r["missing"], json!(["BGPP", "BD"]));
    assert_eq!(r["crypto"], json!([CRYP]));
    assert_eq!(r["unknown"], json!(["reference.png"]));
    assert_eq!((r["sceneFound"].clone(), r["projectFound"].clone()), (Value::Bool(true), Value::Bool(true)));

    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project.items.len(), before, "one undo step takes it all back");
    assert!(ids.iter().all(|i| item(&s, *i).parent.is_none()));
}

#[test]
fn what_is_not_found_is_written_for_the_user_to_paste_over_and_the_folders_are_made() {
    let mut s = Session::default();
    s.execute("prefs.set", json!({"key": "project.quickCompMaker", "value": "Nok"})).unwrap();
    let a = footage(&mut s, "All_v2.png", 1000, 500);
    let v = footage(&mut s, "VFX - smoke.png", 1000, 500);
    s.state.project_selection = vec![effectcraft_project::ItemId(a), effectcraft_project::ItemId(v)];
    let r = s.execute("file.erQuickComp", json!({"date": "010227"})).unwrap();
    assert_eq!(r["name"], "<Scene>_EC_<Project>_010227_(1)Nok");
    assert_eq!((r["sceneFound"].clone(), r["projectFound"].clone()), (Value::Bool(false), Value::Bool(false)));
    let main = r["comp"].as_u64().unwrap();
    let c = s.project.comp(effectcraft_project::ItemId(main)).unwrap();
    assert_eq!((c.width, c.height), (1000, 500), "sized as the footage");
    assert_eq!(layer_names(&s, main), ["ALL", "VFX"]);
    assert_eq!(parent_name(&s, r["precomps"][0]["comp"].as_u64().unwrap()).as_deref(), Some("comp"));
    assert_eq!(parent_name(&s, a).as_deref(), Some("Ft"));
    assert_eq!(s.state.project_selection, vec![effectcraft_project::ItemId(main)]);
}

#[test]
fn nothing_recognised_changes_nothing() {
    let mut s = Session::default();
    let x = footage(&mut s, "reference.png", 100, 100);
    let before = s.project.items.len();
    let e = s.execute("file.erQuickComp", json!({"items": [x]})).unwrap_err().to_string();
    assert!(e.contains("reference.png"), "{e}");
    assert_eq!(s.project.items.len(), before);
}

#[test]
fn the_date_is_today_in_local_time_as_mmddyy() {
    let t = crate::cmd::er_quick_comp::today();
    assert_eq!(t.len(), 6);
    assert_eq!(t, chrono::Local::now().format("%m%d%y").to_string());
}
