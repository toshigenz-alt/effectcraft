//! Local: a Collect Files folder moved or copied elsewhere still finds its footage (open and
//! File ▸ Import ▸ EffectCraft Project fall back to the `(Footage)` folder beside the project),
//! and Collect Files names footage relative to the collected project.

use effectcraft_project::{Footage, FootageKind, ItemKind, Project};
use serde_json::json;

use crate::Session;

fn out_dir(name: &str) -> std::path::PathBuf {
    let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-out").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    // Without `..`: the paths compared below are the ones the session resolved.
    std::path::absolute(&d).unwrap()
}

fn add(s: &mut Session, name: &str, path: &str) {
    let p = std::sync::Arc::make_mut(&mut s.project);
    p.add_item(name, Default::default(), None, ItemKind::Footage(Footage { path: path.into(), kind: FootageKind::Data, ..Default::default() }));
}

fn path_of(p: &Project, name: &str) -> String {
    match &p.items.values().find(|i| i.name == name).unwrap().kind {
        ItemKind::Footage(f) => f.path.clone(),
        _ => panic!("{name} is not footage"),
    }
}

fn same(a: &str, b: &std::path::Path) -> bool {
    std::path::Path::new(a) == b
}

/// A project saved in `new/` whose footage names the folder it was collected into, `old/`, which
/// is gone (as Collect Files wrote it: `/` after the folder, whatever the folder used). The
/// files are now in `new/(Footage)` and `new/`. Returns (project path, the `new` folder).
fn moved_project(dir: &std::path::Path) -> (String, std::path::PathBuf) {
    let old = dir.join("old place").to_string_lossy().to_string();
    let new = dir.join("new place");
    std::fs::create_dir_all(new.join("(Footage)")).unwrap();
    std::fs::write(new.join("(Footage)").join("clip (1).json"), b"{}").unwrap();
    std::fs::write(new.join("loose.json"), b"{}").unwrap();
    let kept = dir.join("kept.json");
    std::fs::write(&kept, b"{}").unwrap();
    let mut s = Session::default();
    add(&mut s, "clip", &format!("{old}/(Footage)/clip (1).json"));
    add(&mut s, "loose", &format!("{old}/loose.json"));
    add(&mut s, "gone", &format!("{old}/(Footage)/gone.json"));
    add(&mut s, "kept", &kept.to_string_lossy());
    let path = new.join("Moved.ecproj").to_string_lossy().to_string();
    s.execute("file.saveAs", json!({"path": path})).unwrap();
    (path, new)
}

#[test]
fn open_finds_footage_of_a_moved_collect_folder() {
    let dir = out_dir("moved-collect-open");
    let (path, new) = moved_project(&dir);
    let mut s = Session::default();
    let r = s.execute("file.open", json!({"path": path})).unwrap();
    assert_eq!(r["foundInCollectFolder"], json!(2), "{r}");
    assert!(same(&path_of(&s.project, "clip"), &new.join("(Footage)").join("clip (1).json")), "{}", path_of(&s.project, "clip"));
    assert!(same(&path_of(&s.project, "loose"), &new.join("loose.json")), "{}", path_of(&s.project, "loose"));
    assert!(path_of(&s.project, "gone").contains("old place"), "a file found nowhere keeps its stored path");
    assert!(same(&path_of(&s.project, "kept"), &dir.join("kept.json")), "footage that is there is not moved");
    // The footage reads through the session's stored-path rules.
    let clip = path_of(&s.project, "clip");
    assert!(s.stored_paths.exists(s.services.as_ref(), &clip));
}

#[test]
fn import_project_finds_footage_of_a_moved_collect_folder() {
    let dir = out_dir("moved-collect-import");
    let (path, new) = moved_project(&dir);
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "Show", "width": 64, "height": 64, "frameRate": 30, "duration": 1})).unwrap();
    let r = s.execute("file.importProject", json!({"path": path})).unwrap();
    assert_eq!(r["foundInCollectFolder"], json!(2), "{r}");
    let missing: Vec<String> = serde_json::from_value(r["missingMedia"].clone()).unwrap();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].ends_with("gone.json"), "{missing:?}");
    assert!(same(&path_of(&s.project, "clip"), &new.join("(Footage)").join("clip (1).json")));
}

#[test]
fn collect_files_names_footage_relative_to_the_collected_project() {
    let dir = out_dir("collect-relative");
    std::fs::write(dir.join("src.json"), b"{}").unwrap();
    let mut s = Session::default();
    add(&mut s, "src", &dir.join("src.json").to_string_lossy());
    let folder = dir.join("collected").to_string_lossy().to_string();
    let r = s.execute("file.collectFiles", json!({"folder": folder})).unwrap();
    let out = r["project"].as_str().unwrap().to_string();
    let saved = Project::from_json(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(path_of(&saved, "src"), "(Footage)/src.json");
    // Moved elsewhere, the collected project still finds its footage.
    let moved = dir.join("moved");
    std::fs::rename(dir.join("collected"), &moved).unwrap();
    let moved_project = moved.join(std::path::Path::new(&out).file_name().unwrap());
    let mut t = Session::default();
    t.execute("file.open", json!({"path": moved_project.to_string_lossy()})).unwrap();
    let found = path_of(&t.project, "src");
    assert!(std::path::Path::new(&found).exists(), "{found}");
    assert!(std::path::Path::new(&found).starts_with(&moved), "{found}");
}
