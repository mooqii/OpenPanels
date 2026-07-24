fn assert_canvas_insert_procedure_contract(payload: &Value) {
    assert_eq!(payload["target"]["kind"], "panel");
    assert_eq!(payload["target"]["moduleKey"], "canvas-document");
    assert!(payload["skills"][0]["body"]
        .as_str()
        .unwrap()
        .contains("current CLI is authoritative"));
    assert_eq!(
        payload["skills"][0]["references"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(payload["skills"][0]["references"][1]["body"]
        .as_str()
        .unwrap()
        .contains("Insert An Existing Canvas Image"));
    assert_eq!(
        payload["executionContract"]["completion"]["kind"],
        "command-response"
    );
    assert!(payload["executionContract"]["artifactInputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|input| input["commandIntent"] == "canvas.image.create"));
}

fn assert_complete_procedure_package(procedure: &str, envelope: &Value) {
    if procedure == "wiki-space.query" {
        assert_eq!(
            envelope["data"]["agentProcedure"]["localSkill"]["mode"],
            "none"
        );
        assert!(envelope["data"]["skills"]
            .as_array()
            .unwrap()
            .iter()
            .all(|skill| skill["role"] != "selected-portable"));
        assert!(envelope["data"]["skills"][0]["references"]
            .as_array()
            .unwrap()
            .iter()
            .all(|reference| reference["body"].as_str().is_some()));
    }
    if procedure.starts_with("my-document.") {
        assert_eq!(envelope["data"]["target"]["kind"], "module");
        assert_ne!(
            envelope["data"]["blockers"]
                .as_array()
                .unwrap()
                .first()
                .map(|blocker| &blocker["code"]),
            Some(&json!("target_panel_required"))
        );
    }
    if procedure == "canvas.image.generate" {
        assert_eq!(
            envelope["data"]["executionContract"]["completion"]["kind"],
            "operation"
        );
        assert_eq!(
            envelope["data"]["executionContract"]["completion"]["successIntent"],
            "operation.complete"
        );
        assert!(envelope["data"]["executionContract"]["artifactInputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input["commandIntent"] == "operation.complete"));
    }
    assert_eq!(envelope["actions"]["required"], json!([]));
    assert!(envelope["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .all(|action| action["intent"] != "agent.catalog"));
}
