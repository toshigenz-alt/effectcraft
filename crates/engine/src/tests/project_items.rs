//! Project panel item edits: move into folders, rename, label, comment, with undo.

use effectcraft_project::{ItemId, LayerId};
use serde_json::json;

use crate::Session;

#[test]
fn move_into_folder_rename_label_comment_with_undo() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "A", "width": 64, "height": 64, "frameRate": 24, "duration": 1})).unwrap();
    let comp = s.active_comp_id().unwrap();
    let f = ItemId(s.execute("project.newFolder", json!({"name": "Shots"})).unwrap()["item"].as_u64().unwrap());
    let g = ItemId(s.execute("project.newFolder", json!({"name": "Inner", "parent": f.0})).unwrap()["item"].as_u64().unwrap());

    s.execute("project.move", json!({"items": [comp.0], "folder": "Shots"})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().parent, Some(f));
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().parent, None);
    s.execute("edit.redo", json!({})).unwrap();
    // Out of the folder (to the root) via the selection.
    s.execute("project.select", json!({"items": [comp.0]})).unwrap();
    s.execute("project.move", json!({"folder": null})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().parent, None);
    // A folder can't go into itself or its descendants.
    assert!(s.execute("project.move", json!({"items": [f.0], "folder": g.0})).is_err());
    assert!(s.execute("project.move", json!({"items": [comp.0], "folder": comp.0})).is_err());

    s.execute("project.rename", json!({"item": comp.0, "name": "Hero"})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().name, "Hero");
    assert!(s.execute("project.rename", json!({"item": comp.0, "name": "  "})).is_err());
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().name, "A");

    s.execute("project.setLabel", json!({"items": [comp.0], "label": "Pink"})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().label.name(), "Pink");
    s.execute("project.setComment", json!({"item": comp.0, "comment": "final"})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().comment, "final");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project.item(comp).unwrap().comment, "");
}

#[test]
fn delete_and_duplicate_items() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "Inner", "width": 64, "height": 64, "duration": 1})).unwrap();
    let inner = s.active_comp_id().unwrap();
    s.execute("layer.newNull", json!({})).unwrap();
    s.execute("comp.new", json!({"name": "Outer", "width": 64, "height": 64, "duration": 1})).unwrap();
    let outer = s.active_comp_id().unwrap();
    s.execute("layer.addItem", json!({"item": inner.0})).unwrap();
    s.execute("renderQueue.add", json!({"comp": inner.0})).unwrap();
    // Duplicate: a new comp with fresh layer ids.
    let r = s.execute("project.duplicate", json!({"items": [inner.0]})).unwrap();
    let dup = ItemId(r["items"][0].as_u64().unwrap());
    assert_eq!(s.project.item(dup).unwrap().name, "Inner 2");
    let a = s.project.comp(inner).unwrap().layers[0].id;
    let b = s.project.comp(dup).unwrap().layers[0].id;
    assert_ne!(a, b);
    // Delete: layers that use the item and its render items go too; undoable.
    s.execute("project.delete", json!({"items": ["Inner"]})).unwrap();
    assert!(s.project.item(inner).is_none());
    assert!(s.project.comp(outer).unwrap().layers.is_empty());
    assert!(s.project.render_queue.is_empty());
    s.undo();
    assert!(s.project.item(inner).is_some());
    assert_eq!(s.project.comp(outer).unwrap().layers.len(), 1);
    // Folders take their contents.
    let f = ItemId(s.execute("project.newFolder", json!({"name": "F"})).unwrap()["item"].as_u64().unwrap());
    s.execute("project.move", json!({"items": [dup.0], "folder": f.0})).unwrap();
    s.execute("project.delete", json!({"items": [f.0]})).unwrap();
    assert!(s.project.item(dup).is_none());
}

/// A new layer takes its Project item's label (#153); layers made before keep their own when the
/// item's label changes. Layers with no item keep their type's default.
#[test]
fn new_layers_take_their_items_label() {
    use crate::color::Label;
    use effectcraft_project::{Footage, FootageKind, ItemKind};
    let mut s = Session::default();
    let inner = s.execute("comp.new", json!({"name": "Inner", "width": 64, "height": 64, "duration": 1})).unwrap()["comp"].as_u64().unwrap();
    s.execute("comp.new", json!({"name": "Main", "width": 64, "height": 64, "duration": 1})).unwrap();
    s.execute("file.importPlaceholder", json!({"name": "Clip"})).unwrap();
    let clip = s.state.project_selection[0].0;
    let model = std::sync::Arc::make_mut(&mut s.project)
        .add_item("Model", Label::Aqua, None, ItemKind::Footage(Footage { kind: FootageKind::Model, path: "missing.glb".into(), ..Default::default() }))
        .0;
    let solid = s.execute("layer.newSolid", json!({"name": "Solid", "color": "#406080"})).unwrap()["layer"].as_u64().unwrap();
    let solid_item = s.active_comp().unwrap().layer(LayerId(solid)).unwrap().source.item().unwrap().0;
    let label = |s: &Session, layer: &serde_json::Value| s.active_comp().unwrap().layer(LayerId(layer["layer"].as_u64().unwrap())).unwrap().label;
    for (item, lab) in [(inner, "Blue"), (clip, "Green"), (model, "Orange"), (solid_item, "Fuchsia")] {
        let before = s.execute("layer.addItem", json!({"item": item})).unwrap();
        s.execute("project.setLabel", json!({"items": [item], "label": lab})).unwrap();
        let after = s.execute("layer.addItem", json!({"item": item})).unwrap();
        assert_eq!(label(&s, &after).name(), lab, "a new layer takes item {item}'s label");
        assert_ne!(label(&s, &before).name(), lab, "a layer made before keeps its own");
    }
    let text = s.execute("layer.newText", json!({"text": "Hi"})).unwrap();
    assert_eq!(label(&s, &text), Label::Red, "no item: the type's default");
}

#[test]
fn property_group_edits() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "C", "width": 64, "height": 64, "duration": 1})).unwrap();
    let l = s.execute("layer.newSolid", json!({})).unwrap()["layer"].as_u64().unwrap();
    let a = s.execute("effect.apply", json!({"layer": l, "effect": "Gaussian Blur"})).unwrap()["effects"][0].as_u64().unwrap();
    let b = s.execute("effect.apply", json!({"layer": l, "effect": "Invert"})).unwrap()["effects"][0].as_u64().unwrap();
    let fx = |s: &Session| -> Vec<(u64, String, bool)> {
        s.active_comp().unwrap().layers[0].props.sub("effects").unwrap().groups().map(|g| (g.uid, g.name.clone(), g.enabled)).collect()
    };
    s.execute("prop.renameGroup", json!({"layer": l, "prop": a, "name": "Soft"})).unwrap();
    s.execute("prop.setGroupEnabled", json!({"layer": l, "prop": a, "value": false})).unwrap();
    s.execute("prop.moveGroup", json!({"layer": l, "prop": b, "index": 1})).unwrap();
    let d = s.execute("prop.duplicateGroup", json!({"layer": l, "prop": a})).unwrap()["prop"].as_u64().unwrap();
    let v = fx(&s);
    assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), [b, a, d]);
    assert_eq!(v[1].1, "Soft");
    assert!(!v[1].2);
    s.execute("prop.removeGroup", json!({"layer": l, "prop": a})).unwrap();
    assert_eq!(fx(&s).len(), 2);
    // Fixed groups can't be removed.
    let tr = s.active_comp().unwrap().layers[0].props.sub("transform").unwrap().uid;
    assert!(s.execute("prop.removeGroup", json!({"layer": l, "prop": tr})).is_err());
}

/// Delete with a text animator, mask or shape group selected removes just those groups, in one
/// undo step, and keeps their layers (#151). A fixed group selected still deletes the layer.
#[test]
fn delete_removes_selected_groups_not_their_layer() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "C", "width": 64, "height": 64, "duration": 1})).unwrap();
    let t = s.execute("layer.newText", json!({"text": "ABCD"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.addTextAnimator", json!({"layer": t, "properties": ["tracking"]})).unwrap();
    let sh = s.execute("layer.newShape", json!({"kind": "star"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.addMask", json!({"layer": sh, "shape": "ellipse"})).unwrap();
    let groups = |s: &Session, l: u64, path: &str| -> Vec<u64> {
        s.active_comp().unwrap().layer(LayerId(l)).map_or(vec![], |l| l.props.group(path).unwrap().groups().map(|g| g.uid).collect())
    };
    let anim = groups(&s, t, "text/animators")[0];
    let (mask, shape) = (groups(&s, sh, "masks")[0], groups(&s, sh, "contents")[0]);
    let delete = |s: &mut Session| {
        let undo = s.history.undo.len();
        s.execute("edit.clear", json!({})).unwrap();
        assert_eq!(s.history.undo.len(), undo + 1, "one undo step");
    };

    s.execute("prop.select", json!({"layer": t, "prop": anim, "selectKeys": false})).unwrap();
    let before = s.project.clone();
    delete(&mut s);
    assert!(groups(&s, t, "text/animators").is_empty(), "the animator is gone");
    assert_eq!(s.active_comp().unwrap().layers.len(), 2, "its layer stays");
    assert!(s.state.selected_props.is_empty());
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(s.project, before);

    s.execute("prop.select", json!({"layer": sh, "prop": mask, "selectKeys": false})).unwrap();
    s.execute("prop.select", json!({"layer": sh, "prop": shape, "selectKeys": false, "add": true})).unwrap();
    delete(&mut s);
    assert!(groups(&s, sh, "masks").is_empty());
    assert!(!groups(&s, sh, "contents").contains(&shape));
    assert_eq!(s.active_comp().unwrap().layers.len(), 2);

    let tr = s.active_comp().unwrap().layer(LayerId(t)).unwrap().props.sub("transform").unwrap().uid;
    s.execute("prop.select", json!({"layer": t, "prop": tr, "selectKeys": false})).unwrap();
    delete(&mut s);
    assert!(s.active_comp().unwrap().layer(LayerId(t)).is_none(), "a fixed group selected: the layer goes");
}

/// #1061: text animators, masks and shape groups rename like effects, with undo; fixed groups and
/// empty names are refused.
#[test]
fn text_animators_masks_and_shape_groups_rename() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "C", "width": 64, "height": 64, "duration": 1})).unwrap();
    let t = s.execute("layer.newText", json!({"text": "ABCD"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.addTextAnimator", json!({"layer": t, "properties": ["tracking"]})).unwrap();
    let sh = s.execute("layer.newShape", json!({"kind": "star"})).unwrap()["layer"].as_u64().unwrap();
    s.execute("layer.addMask", json!({"layer": sh, "shape": "ellipse"})).unwrap();
    let uid = |s: &Session, l: u64, path: &str| s.active_comp().unwrap().layer(LayerId(l)).unwrap().props.group(path).unwrap().groups().next().unwrap().uid;
    let name = |s: &Session, l: u64, u: u64| s.active_comp().unwrap().layer(LayerId(l)).unwrap().props.find_group(u).unwrap().name.clone();
    for (l, path, new) in [(t, "text/animators", "Wiggle In"), (sh, "masks", "Reveal"), (sh, "contents", "Star Body")] {
        let u = uid(&s, l, path);
        let old = name(&s, l, u);
        s.execute("prop.renameGroup", json!({"layer": l, "prop": u, "name": new})).unwrap();
        assert_eq!(name(&s, l, u), new, "{path}");
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(name(&s, l, u), old, "{path} undone");
        assert!(s.execute("prop.renameGroup", json!({"layer": l, "prop": u, "name": "  "})).is_err());
    }
}

/// `project.usedIn`: the Project panel's "used N times" and its ▾ list of comps, by name.
#[test]
fn used_in_counts_layers_across_comps() {
    let mut s = Session::default();
    s.execute("comp.new", json!({"name": "Inner", "width": 64, "height": 64, "duration": 1})).unwrap();
    let inner = s.active_comp_id().unwrap();
    s.execute("comp.new", json!({"name": "Outer", "width": 64, "height": 64, "duration": 1})).unwrap();
    let outer = s.active_comp_id().unwrap();
    s.execute("layer.addItem", json!({"item": inner.0})).unwrap();
    s.execute("layer.addItem", json!({"item": inner.0})).unwrap();
    let r = s.execute("project.usedIn", json!({"item": "Inner"})).unwrap();
    assert_eq!(r["count"], 2);
    assert_eq!(r["comps"], json!([{"id": outer.0, "name": "Outer", "layers": 2}]));
    // Default: the selected item; an unused one is used 0 times.
    s.execute("project.select", json!({"items": [outer.0]})).unwrap();
    let r = s.execute("project.usedIn", json!({})).unwrap();
    assert_eq!((r["item"].as_u64(), r["count"].as_u64()), (Some(outer.0), Some(0)));
    assert_eq!(r["comps"], json!([]));
    assert!(s.execute("project.usedIn", json!({"item": "Nope"})).is_err());
}

/// Local: `project.newFolder {items}` (items dropped on the Project panel's Create a new Folder
/// button) puts them in a new folder in one undo step: where they all are, else at the root.
#[test]
fn new_folder_with_items_moves_them_in_one_undo_step() {
    let mut s = Session::default();
    let id = |v: serde_json::Value| ItemId(v["item"].as_u64().unwrap());
    let shots = id(s.execute("project.newFolder", json!({"name": "Shots"})).unwrap());
    let a = id(s.execute("file.importPlaceholder", json!({"name": "a.png", "width": 8, "height": 8})).unwrap());
    let b = id(s.execute("file.importPlaceholder", json!({"name": "b.png", "width": 8, "height": 8})).unwrap());
    let c = id(s.execute("file.importPlaceholder", json!({"name": "c.png", "width": 8, "height": 8})).unwrap());
    s.execute("project.move", json!({"items": [a.0, b.0], "folder": shots.0})).unwrap();
    let parent = |s: &Session, i: ItemId| s.project.item(i).unwrap().parent;

    // Both in Shots: the new folder goes into Shots, holding them.
    let f = id(s.execute("project.newFolder", json!({"items": [a.0, b.0]})).unwrap());
    assert!(s.project.item(f).unwrap().is_folder());
    assert_eq!(parent(&s, f), Some(shots));
    assert_eq!((parent(&s, a), parent(&s, b)), (Some(f), Some(f)));
    assert_eq!(s.state.project_selection, vec![f], "the new folder is selected");
    // One undo removes the folder and puts both back.
    s.execute("edit.undo", json!({})).unwrap();
    assert!(s.project.item(f).is_none());
    assert_eq!((parent(&s, a), parent(&s, b)), (Some(shots), Some(shots)));

    // From two places (Shots and the root): the folder goes to the root.
    let g = id(s.execute("project.newFolder", json!({"items": [a.0, c.0]})).unwrap());
    assert_eq!(parent(&s, g), None);
    assert_eq!((parent(&s, a), parent(&s, c)), (Some(g), Some(g)));

    // A folder can't go into a new folder made inside itself; nothing changes.
    let before = s.project.items.len();
    assert!(s.execute("project.newFolder", json!({"items": [shots.0], "parent": shots.0})).is_err());
    assert_eq!(s.project.items.len(), before);
    // Unknown ids are refused.
    assert!(s.execute("project.newFolder", json!({"items": [999_999]})).is_err());
    assert_eq!(s.project.items.len(), before);
    // Without `items` it is the plain New Folder.
    let plain = id(s.execute("project.newFolder", json!({})).unwrap());
    assert_eq!(parent(&s, plain), None);
}
