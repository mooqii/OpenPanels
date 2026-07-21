pub fn agent_cli_executable() -> String {
    std::env::var("MYOPENPANELS_CLI")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(checkout_debug_cli)
        .unwrap_or_else(|| "myopenpanels".to_owned())
}

pub fn agent_cli_shell_word() -> String {
    shell_quote(&agent_cli_executable())
}

fn shell_quote(executable: &str) -> String {
    if executable
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        executable.to_owned()
    } else {
        format!("'{}'", executable.replace('\'', "'\\''"))
    }
}

fn checkout_debug_cli() -> Option<String> {
    let path = std::env::current_exe().ok()?;
    let executable_name = path.file_stem()?.to_str()?;
    let path_text = path.to_string_lossy();
    (executable_name == "myopenpanels"
        && (path_text.contains("/target/debug/") || path_text.contains("\\target\\debug\\")))
    .then(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_word_quotes_paths_with_spaces() {
        assert_eq!(
            shell_quote("/checkout/My OpenPanels/scripts/myopenpanels-dev"),
            "'/checkout/My OpenPanels/scripts/myopenpanels-dev'"
        );
    }

    #[test]
    fn test_binary_is_not_mistaken_for_the_checkout_cli() {
        assert_eq!(checkout_debug_cli(), None);
    }
}
