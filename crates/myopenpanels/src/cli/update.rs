use super::support::has_flag;
use super::*;

pub(super) fn run_update_command(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    if subcommand == Some("help") || has_flag(parsed, "help") {
        write_text(stdout, &update_help_text())?;
        return Ok(());
    }

    if has_flag(parsed, "check") || subcommand == Some("check") {
        let payload = check_for_update(VERSION, false)?;
        let text = update_check_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    if subcommand == Some("download") {
        let payload = download_update(VERSION)?;
        let text = update_download_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    if subcommand.is_none() || subcommand == Some("install") {
        let payload = install_update(VERSION)?;
        let text = update_install_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    Err(CliError::new(format!(
        "Unknown update command: {}",
        subcommand.unwrap_or_default()
    )))
}

pub(super) fn update_check_text(payload: &UpdateCheckPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.update_available {
        if payload.asset_available {
            format!(
                "Update available: myopenpanels {} -> {latest}. Run `myopenpanels update` to install.",
                payload.current_version
            )
        } else {
            format!(
                "Update available: myopenpanels {} -> {latest}, but no asset exists for {}.",
                payload.current_version, payload.target
            )
        }
    } else {
        format!("myopenpanels is up to date ({latest}).")
    }
}

pub(super) fn update_install_text(payload: &UpdateInstallPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.updated {
        format!(
            "Updated myopenpanels {} -> {latest}. Run `myopenpanels studio start --project-dir <project> --format json` to restart Studio, then navigate to the returned embeddedBrowserUrl.",
            payload.current_version
        )
    } else {
        format!("myopenpanels is already up to date ({latest}).")
    }
}

pub(super) fn update_download_text(payload: &UpdateDownloadPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.downloaded {
        format!("Downloaded myopenpanels {latest}.")
    } else if payload.update_available {
        format!(
            "Update available: myopenpanels {} -> {latest}, but it was not downloaded.",
            payload.current_version
        )
    } else {
        format!("myopenpanels is already up to date ({latest}).")
    }
}

pub(super) fn update_help_text() -> String {
    format!(concat!(
        "myopenpanels update [check|install] [options]\n\n",
        "Commands:\n",
        "  update                    Download, verify, and install the latest GitHub Releases binary\n",
        "  update install            Same as `update`\n",
        "  update download           Download and cache the latest binary without installing it\n",
        "  update check              Check whether a newer GitHub Releases binary exists\n\n",
        "Options:\n",
        "  --check                   Same as `update check`\n",
        "  --format json             Emit stable JSON output\n\n",
        "Environment:\n",
        "  MYOPENPANELS_UPDATE_MANIFEST_URL  Override the release manifest URL\n",
        "  MYOPENPANELS_UPDATE_CACHE_DIR     Override the 24-hour update check cache directory\n",
        "  MYOPENPANELS_DISABLE_UPDATE_CHECK Disable opportunistic 24-hour update checks\n\n",
        "Default manifest:\n",
        "{}\n",
    ), DEFAULT_MANIFEST_URL)
}
