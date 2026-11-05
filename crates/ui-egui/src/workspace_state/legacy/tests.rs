use super::*;

fn stack(count: usize) -> OldNode {
    OldNode::Stack {
        entries: (0..count).map(|_| OldStackEntry { panel: Some(PanelKind::Info), panels: Vec::new(), active: 0, open: true, height: None }).collect(),
    }
}

#[test]
fn stacked_frames_respect_the_remaining_tree_depth() {
    let limit = craft_ui::docking::MAX_DEPTH;
    assert!(node(stack(limit + 1), 0).is_ok());
    assert!(node(stack(limit + 2), 0).is_err());
    assert!(node(stack(1), limit).is_ok());
    assert!(node(stack(2), limit).is_err());
    assert!(node(stack(limit), 2).is_err());
}

#[test]
fn oversized_legacy_stack_load_does_not_abort() {
    const CHILD: &str = "EFFECTCRAFT_TEST_LEGACY_STACK_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let entry = serde_json::json!({"panel": "Info", "open": true});
        let value = serde_json::json!({"workspaces": {"Oversized": {"Stack": {"entries": vec![entry; 20_000]}}}});
        assert!(serde_json::to_vec(&value).unwrap().len() < super::super::MAX_BYTES);
        assert!(migrate(value, &UiState::default()).is_err());
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "workspace_state::legacy::tests::oversized_legacy_stack_load_does_not_abort", "--nocapture"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

// Local: column widths saved by our earlier builds survive the move to version 2.
#[test]
fn version_1_column_widths_carry_over() {
    let dock = serde_json::json!({"Stack": {"entries": [{"panel": "Info", "open": true}]}});
    let value = serde_json::json!({
        "version": 1, "workspace": "Default", "dock": dock, "floating": [], "focused": "Info", "maximized": null,
        "saved_workspaces": {}, "saved_floating": {},
        "timeline_col_widths": {"name": 280.0}, "timeline_left_w": 620.0, "project_col_widths": {"name": 220.0},
    });
    let (snapshot, _) = migrate(value, &UiState::default()).unwrap();
    assert_eq!(snapshot.timeline_col_widths.get("name"), Some(&280.0));
    assert_eq!(snapshot.timeline_left_w, Some(620.0));
    assert_eq!(snapshot.project_col_widths.get("name"), Some(&220.0));
}
