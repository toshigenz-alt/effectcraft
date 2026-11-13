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

/// Local: Collect Files keeps the Project panel's folders in `(Footage)`, and each image sequence
/// gets a folder of its own named after the folder its frames were in — two sequences with the
/// same frame names (both `render/0001.png`) no longer share one folder. Folder names a file
/// system can't take (or would read as a way out) are made safe; the collected folder still works
/// after it is moved.
#[test]
fn collect_files_keeps_project_folders_and_gives_each_sequence_a_folder() {
    let dir = out_dir("collect-folders");
    let file = |rel: &str| {
        let p = dir.join("src").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"{}").unwrap();
        p.to_string_lossy().into_owned()
    };
    let mut s = Session::default();
    let p = std::sync::Arc::make_mut(&mut s.project);
    let folder = |p: &mut Project, name: &str, parent| Some(p.add_item(name, Default::default(), parent, ItemKind::Folder));
    let still = |path: String| ItemKind::Footage(Footage { path, kind: FootageKind::Data, ..Default::default() });
    let seq = |frames: Vec<String>| ItemKind::Footage(Footage { path: frames[0].clone(), sequence: frames, kind: FootageKind::Sequence, ..Default::default() });
    let plates = folder(p, "Plates", None);
    let bg = folder(p, "BG", plates);
    p.add_item("logo", Default::default(), bg, still(file("art/logo.json")));
    p.add_item("music", Default::default(), None, still(file("audio/music.json")));
    p.add_item("shot A", Default::default(), plates, seq(vec![file("shotA/render/0001.json"), file("shotA/render/0002.json")]));
    p.add_item("shot B", Default::default(), plates, seq(vec![file("shotB/Render/0001.json"), file("shotB/Render/0002.json")]));
    // Two items reading one file: one copy.
    p.add_item("logo again", Default::default(), bg, still(file("art/logo.json")));
    // A loose file named like a sequence folder sits beside it.
    p.add_item("render", Default::default(), plates, still(file("loose/render")));
    let up = folder(p, "..", None);
    p.add_item("up", Default::default(), up, still(file("u/up.json")));
    let con = folder(p, "con.v1", None);
    let slash = folder(p, "a/b:c ", con);
    p.add_item("deep", Default::default(), slash, still(file("d/deep.json")));
    let footage = folder(p, "(Footage)", None);
    p.add_item("inner", Default::default(), footage, still(file("i/inner.json")));

    let out = dir.join("out");
    let r = s.execute("file.collectFiles", json!({"folder": out.to_string_lossy()})).unwrap();
    assert_eq!(r["errors"], json!([]), "{r}");
    // 10 files: two 2-frame sequences, six single files; "logo again" reads logo.json, copied once.
    assert_eq!(r["copied"], json!(10), "{r}");
    let collected = Project::from_json(&std::fs::read_to_string(r["project"].as_str().unwrap()).unwrap()).unwrap();
    let seq_of = |name: &str| match &collected.items.values().find(|i| i.name == name).unwrap().kind {
        ItemKind::Footage(f) => f.sequence.clone(),
        _ => panic!(),
    };
    for (name, want) in [
        ("logo", "(Footage)/Plates/BG/logo.json"),
        ("logo again", "(Footage)/Plates/BG/logo.json"),
        ("music", "(Footage)/music.json"),
        ("shot A", "(Footage)/Plates/render/0001.json"),
        ("shot B", "(Footage)/Plates/Render 2/0001.json"),
        ("render", "(Footage)/Plates/2_render"),
        ("up", "(Footage)/Folder/up.json"),
        ("deep", "(Footage)/_con.v1/a_b_c/deep.json"),
        ("inner", "(Footage)/_(Footage)/inner.json"),
    ] {
        assert_eq!(path_of(&collected, name), want, "{name}");
        assert!(out.join(want).is_file(), "{want} was written");
    }
    assert_eq!(seq_of("shot B"), ["(Footage)/Plates/Render 2/0001.json", "(Footage)/Plates/Render 2/0002.json"]);
    // Nothing outside the collect folder.
    assert!(!dir.join("Folder").exists() && !dir.join("up.json").exists());
    // Moved elsewhere, every file is found again.
    let moved = dir.join("moved");
    std::fs::rename(&out, &moved).unwrap();
    let project = moved.join(std::path::Path::new(r["project"].as_str().unwrap()).file_name().unwrap());
    let mut t = Session::default();
    t.execute("file.open", json!({"path": project.to_string_lossy()})).unwrap();
    for it in t.project.items.values() {
        if let ItemKind::Footage(f) = &it.kind {
            for q in if f.sequence.is_empty() { vec![f.path.clone()] } else { f.sequence.clone() } {
                assert!(t.stored_paths.exists(t.services.as_ref(), &q), "{}: {q}", it.name);
            }
        }
    }
}
