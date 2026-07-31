const V2EX_NODES_URL: &str = "https://www.v2ex.com/api/nodes/all.json";
const V2EX_NODES_TIMEOUT_SECS: u64 = 10;
const V2EX_NODES_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn v2ex_nodes() -> Result<Value, CliError> {
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(V2EX_NODES_TIMEOUT_SECS))
        .build()
        .get(V2EX_NODES_URL)
        .call()
        .map_err(|_| {
            CliError::with_code(
                "v2ex_nodes_unavailable",
                "V2EX nodes could not be loaded. Try again.",
            )
        })?;
    let payload: Value = serde_json::from_reader(
        std::io::Read::take(response.into_reader(), V2EX_NODES_MAX_BYTES),
    )
    .map_err(|_| {
        CliError::with_code(
            "v2ex_nodes_invalid_response",
            "V2EX returned an unreadable node list.",
        )
    })?;
    normalized_v2ex_nodes(&payload)
}

fn normalized_v2ex_nodes(payload: &Value) -> Result<Value, CliError> {
    let nodes = payload.as_array().ok_or_else(|| {
        CliError::with_code(
            "v2ex_nodes_invalid_response",
            "V2EX returned an unreadable node list.",
        )
    })?;
    let mut normalized = nodes
        .iter()
        .filter_map(|node| {
            let name = node.get("name").and_then(Value::as_str)?;
            let title = node.get("title").and_then(Value::as_str)?;
            if !valid_v2ex_node_name(name) || title.trim().is_empty() {
                return None;
            }
            Some(json!({
                "id": node.get("id").and_then(Value::as_i64).unwrap_or_default(),
                "name": name,
                "title": title,
                "titleAlternative": node
                    .get("title_alternative")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "topics": node.get("topics").and_then(Value::as_i64).unwrap_or_default(),
                "stars": node.get("stars").and_then(Value::as_i64).unwrap_or_default(),
            }))
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        right["stars"]
            .as_i64()
            .cmp(&left["stars"].as_i64())
            .then_with(|| right["topics"].as_i64().cmp(&left["topics"].as_i64()))
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    Ok(json!({ "nodes": normalized }))
}

fn valid_v2ex_node_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn publishing_destination(
    target: PublishingTarget,
    node_name: Option<&str>,
    node_title: Option<&str>,
) -> Result<Value, CliError> {
    if target.platform != "v2ex" {
        return Ok(Value::Null);
    }
    let node_name = node_name.map(str::trim).filter(|value| valid_v2ex_node_name(value));
    let node_title = node_title.map(str::trim).filter(|value| !value.is_empty());
    let (Some(node_name), Some(node_title)) = (node_name, node_title) else {
        return Err(CliError::with_code(
            "v2ex_node_required",
            "Choose a V2EX node before publishing.",
        ));
    };
    if node_title.chars().count() > 200 {
        return Err(CliError::with_code(
            "v2ex_node_invalid",
            "The selected V2EX node is invalid.",
        ));
    }
    Ok(json!({
        "kind": "v2ex_node",
        "nodeName": node_name,
        "nodeTitle": node_title,
    }))
}

#[cfg(test)]
mod v2ex_tests {
    use super::*;

    #[test]
    fn nodes_are_sanitized_and_sorted_for_the_picker() {
        let payload = json!([
            {
                "id": 1,
                "name": "python",
                "title": "Python",
                "title_alternative": "Python",
                "topics": 20,
                "stars": 5
            },
            {
                "id": 2,
                "name": "create",
                "title": "分享创造",
                "topics": 50,
                "stars": 10
            },
            {
                "id": 3,
                "name": "../unsafe",
                "title": "Unsafe",
                "topics": 100,
                "stars": 100
            }
        ]);
        let result = normalized_v2ex_nodes(&payload).expect("nodes");
        assert_eq!(result["nodes"].as_array().unwrap().len(), 2);
        assert_eq!(result["nodes"][0]["name"], "create");
        assert_eq!(result["nodes"][1]["name"], "python");
    }

    #[test]
    fn v2ex_destination_is_required_and_normalized() {
        let destination =
            publishing_destination(V2EX_TARGET, Some(" create "), Some(" 分享创造 "))
                .expect("destination");
        assert_eq!(destination["nodeName"], "create");
        assert_eq!(destination["nodeTitle"], "分享创造");
        assert_eq!(
            publishing_destination(V2EX_TARGET, Some("../create"), Some("分享创造"))
                .expect_err("unsafe node")
                .code(),
            Some("v2ex_node_required")
        );
        assert_eq!(
            publishing_destination(X_TARGET, None, None).expect("other target"),
            Value::Null
        );
    }
}
