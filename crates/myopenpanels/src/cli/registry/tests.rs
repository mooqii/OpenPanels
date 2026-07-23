#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_every_public_clap_leaf_once() {
        validate().expect("complete command registry");
        let index = catalog(None).expect("catalog index");
        assert!(index["domains"]
            .as_array()
            .is_some_and(|domains| !domains.is_empty()));
        assert!(catalog(Some("studio")).is_none());

        let my_documents = catalog(Some("my-document")).expect("My Documents catalog");
        let commands = my_documents["commands"].as_array().expect("commands");
        let delete = commands
            .iter()
            .find(|command| command["intent"] == "my-document.delete")
            .expect("document delete");
        assert_eq!(delete["risk"], "high-risk-write");
        let allowed = [
            "intent",
            "description",
            "argv",
            "args",
            "risk",
            "target",
            "retry",
        ];
        for command in commands {
            assert!(command
                .as_object()
                .unwrap()
                .keys()
                .all(|key| allowed.contains(&key.as_str())));
            assert_eq!(
                command["argv"].as_array().unwrap().last(),
                Some(&json!("json"))
            );
        }

        for spec in SPECS.iter().filter(|spec| catalog_domain(spec).is_some()) {
            let descriptor = descriptor(spec);
            let argv = descriptor["argv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            assert!(
                matches!(
                    super::super::args::parse(&argv),
                    super::super::args::ParseOutcome::Invocation(_)
                ),
                "catalog argv must parse for {}: {argv:?}",
                spec.intent
            );
        }

        let canvas = catalog(Some("canvas")).expect("canvas catalog");
        let create = canvas["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["intent"] == "canvas.image.create")
            .unwrap();
        assert_eq!(create["target"]["mode"], "panel-kind");
        let selection = canvas["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["intent"] == "canvas.selection.export")
            .unwrap();
        assert_eq!(selection["target"]["mode"], "active-selection");
        for spec in SPECS {
            let mut argv = spec
                .path
                .iter()
                .map(|part| (*part).to_owned())
                .collect::<Vec<_>>();
            argv.push("--help".to_owned());
            assert!(
                matches!(
                    super::super::args::parse(&argv),
                    super::super::args::ParseOutcome::Display(_)
                ),
                "{} must expose Clap help",
                spec.path.join(" ")
            );
        }
    }
}
