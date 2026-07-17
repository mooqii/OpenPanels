#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_json_limits_utf8_depth_arrays_and_objects() {
        let mut object = serde_json::Map::new();
        for index in 0..40 {
            object.insert(format!("field{index:02}"), json!(index));
        }
        object.insert("long".to_owned(), json!("界".repeat(200)));
        object.insert(
            "array".to_owned(),
            Value::Array((0..20).map(|value| json!(value)).collect()),
        );
        object.insert(
            "deep".to_owned(),
            json!({ "one": { "two": { "three": { "four": true } } } }),
        );
        let mut truncated = false;
        let bounded = bounded_json(Value::Object(object), 0, &mut truncated);

        assert!(truncated);
        assert!(serde_json::to_vec(&bounded).expect("json").len() < 1024);
        assert!(bounded.as_object().unwrap().len() <= 32);
        let mut string_truncated = false;
        let string = bounded_json(json!("界".repeat(200)), 0, &mut string_truncated);
        assert!(string_truncated);
        assert!(string.as_str().unwrap().len() <= 256);
        assert!(string
            .as_str()
            .unwrap()
            .is_char_boundary(string.as_str().unwrap().len()));
    }

    #[test]
    fn compact_operation_references_leave_actions_at_the_response_root() {
        let summary = compact_operation_summary(&[json!({
            "id": "operation:1",
            "intent": "canvas.generation.begin",
            "panelKind": "canvas",
            "status": "active",
        })]);

        assert!(summary["items"][0].get("readAction").is_none());
        assert!(summary["items"][0].get("readCommand").is_none());
    }

    #[test]
    fn recommended_domains_skip_panel_kinds_without_agent_commands() {
        assert_eq!(
            recommended_catalog_domains(PanelKind::Typesetting),
            vec!["operation", "panel", "task"]
        );
        assert_eq!(
            recommended_catalog_domains(PanelKind::Wiki),
            vec!["operation", "panel", "task", "wiki"]
        );
    }
}
