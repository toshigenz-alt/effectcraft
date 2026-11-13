//! Local: File ▸ Dependencies ▸ Er Reduce+Collect — Reduce Project to the selected comps, Save
//! over the project's file, then Collect Files into the folder beside it named after it, with no
//! dialog.

use effectcraft_project::{Footage, FootageKind, ItemId, ItemKind, Project};
use serde_json::json;

use crate::Session;

fn out_dir(name: &str) -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-out").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::path::absolute(&d).unwrap()
}

fn add(s: &mut Session, name: &str, path: &std::path::Path) -> u64 {
    std::fs::write(path, b"{}").unwrap();
    let p = std::sync::Arc::make_mut(&mut s.project);
    p.add_item(
        name,
        Default::default(),
        None,
        ItemKind::Footage(Footage {
            path: path.to_string_lossy().into(),
            kind: FootageKind::Still,
            width: 4,
            height: 4,
            has_video: true,
            ..Default::default()
        }),
    )
    .0
}

fn comp(s: &mut Session, name: &str, item: u64) -> u64 {
    let c = s.execute("comp.new", json!({"name": name, "width": 64, "height": 36, "frameRate": 30, "duration": 1})).unwrap()["comp"].as_u64().unwrap();
    s.execute("layer.addItem", json!({"comp": c, "item": item, "time": 0.0})).unwrap();
    c
}

fn names(p: &Project) -> Vec<String> {
    let mut v: Vec<String> = p.items.values().map(|i| i.name.clone()).collect();
    v.sort();
    v
}

fn footage_path(p: &Project, name: &str) -> String {
    match &p.items.values().find(|i| i.name == name).unwrap().kind {
        ItemKind::Footage(f) => f.path.clone(),
        _ => panic!("{name} is not footage"),
    }
}

/// The project file's name has a space and a dot (`Job v1.2`): the folder is named after the
/// whole stem, not cut at the dot.
#[test]
fn reduce_and_collect_reduces_saves_and_collects_beside_the_file() {
    let dir = out_dir("reduce-and-collect");
    let src = dir.join("media");
    std::fs::create_dir_all(&src).unwrap();
    let mut s = Session::default();
    let used = add(&mut s, "used", &src.join("used.png"));
    let unused = add(&mut s, "unused", &src.join("unused.png"));
    let main = comp(&mut s, "Main", used);
    comp(&mut s, "Other", unused);
    let path = dir.join("Job v1.2.ecproj").to_string_lossy().to_string();
    s.execute("file.saveAs", json!({"path": path})).unwrap();
    s.state.project_selection = vec![ItemId(main)];

    let r = s.execute("file.reduceAndCollect", json!({})).unwrap();

    let folder = dir.join("Job v1.2");
    assert_eq!(r["removed"], json!(2), "{r}");
    assert_eq!(r["saved"], json!(path), "{r}");
    assert_eq!(r["folder"], json!(folder.to_string_lossy()), "{r}");
    assert_eq!((r["copied"].clone(), r["errors"].clone()), (json!(1), json!([])), "{r}");
    // The open project is the reduced one, still at its own file, and saved.
    assert_eq!(names(&s.project), ["Main", "used"]);
    assert_eq!(s.path.as_deref(), Some(path.as_str()));
    // Its file holds the reduced project, footage where it was.
    let saved = Project::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(names(&saved), ["Main", "used"]);
    assert_eq!(footage_path(&saved, "used"), src.join("used.png").to_string_lossy());
    // The collected folder: the project named after the file, only the used footage.
    let collected = Project::from_json(&std::fs::read_to_string(folder.join("Job v1.2.ecproj")).unwrap()).unwrap();
    assert_eq!(names(&collected), ["Main", "used"]);
    assert_eq!(footage_path(&collected, "used"), "(Footage)/used.png");
    let copied: Vec<String> = std::fs::read_dir(folder.join("(Footage)")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(copied, ["used.png"]);
    // Reduce is one undo step.
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(names(&s.project), ["Main", "Other", "unused", "used"]);
}

/// Unsaved: there is no file name to collect under, so nothing is reduced.
#[test]
fn reduce_and_collect_needs_a_saved_project_and_a_selected_comp() {
    let dir = out_dir("reduce-and-collect-unsaved");
    let mut s = Session::default();
    let used = add(&mut s, "used", &dir.join("used.png"));
    let main = comp(&mut s, "Main", used);
    comp(&mut s, "Other", used);
    s.state.project_selection = vec![ItemId(main)];
    let e = s.execute("file.reduceAndCollect", json!({})).unwrap_err().to_string();
    assert!(e.contains("save the project first"), "{e}");
    assert_eq!(s.project.items.len(), 3);
    // Saved, but no comp selected.
    s.execute("file.saveAs", json!({"path": dir.join("P.ecproj").to_string_lossy()})).unwrap();
    s.state.project_selection = vec![ItemId(used)];
    let e = s.execute("file.reduceAndCollect", json!({})).unwrap_err().to_string();
    assert!(e.contains("select compositions"), "{e}");
    assert_eq!(s.project.items.len(), 3);
    assert!(!dir.join("P").exists());
}

#[test]
fn collect_folder_is_beside_the_file_named_after_it() {
    // Compared as paths: on Windows the name is joined with `\`, whatever the folder used.
    let f = |p: &str| crate::doc_path::collect_folder_for(p).map(std::path::PathBuf::from);
    assert_eq!(f("D:/Jobs/Intro v1.2.ecproj"), Some("D:/Jobs/Intro v1.2".into()));
    assert_eq!(f("/jobs/a.b.c.ecproj"), Some("/jobs/a.b.c".into()));
    assert_eq!(f(""), None);
}
