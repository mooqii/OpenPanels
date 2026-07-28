#[derive(Debug, Clone)]
struct GithubSkillSource {
    owner: String,
    repo: String,
    revision: String,
    subpath: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteSkillSource {
    archive_url: String,
    github: Option<GithubSkillSource>,
    label: &'static str,
    skill_selector: Option<String>,
    subpath: Option<String>,
    provenance: SkillProvenanceSource,
}

#[derive(Debug, Deserialize)]
struct GithubContentEntry {
    #[serde(rename = "type")]
    entry_type: String,
    path: String,
    #[serde(default)]
    size: usize,
    #[serde(rename = "download_url")]
    download_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GithubContentsResponse {
    Entries(Vec<GithubContentEntry>),
    Entry(GithubContentEntry),
}

fn prepare_github_subdirectory(
    source: RemoteSkillSource,
    github: &GithubSkillSource,
) -> Result<PreparedRemoteArchive, CliError> {
    let subpath = github
        .subpath
        .as_deref()
        .ok_or_else(|| invalid_skill_import("The GitHub Skill subdirectory is missing."))?;
    let requested = safe_import_relative_path(subpath)?;
    let temporary = tempfile::tempdir().map_err(to_cli_error)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();
    let mut directories = vec![subpath.to_owned()];
    let mut entry_count = 0usize;
    let mut total_size = 0usize;

    while let Some(directory) = directories.pop() {
        let response = agent
            .get(&github_contents_api_url(github, &directory))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "MyOpenPanels-Skill-Importer")
            .call()
            .map_err(|error| {
                CliError::with_code(
                    "skill_source_unavailable",
                    format!("Could not list the GitHub Skill directory: {error}"),
                )
            })?
            .into_json::<GithubContentsResponse>()
            .map_err(|error| {
                CliError::with_code(
                    "skill_source_unavailable",
                    format!("GitHub returned an invalid Skill directory listing: {error}"),
                )
            })?;
        let entries = match response {
            GithubContentsResponse::Entries(entries) => entries,
            GithubContentsResponse::Entry(entry) => vec![entry],
        };

        for entry in entries {
            let relative = safe_import_relative_path(&entry.path)?;
            if !relative.starts_with(&requested) {
                return Err(invalid_skill_import(
                    "GitHub returned a file outside the requested Skill directory.",
                ));
            }
            if matches!(entry.entry_type.as_str(), "dir" | "file") {
                entry_count += 1;
                if entry_count > 512 {
                    return Err(CliError::with_code(
                        "skill_package_too_large",
                        "The GitHub Skill directory contains too many files.",
                    ));
                }
            }
            match entry.entry_type.as_str() {
                "dir" => directories.push(entry.path),
                "file" => {
                    total_size = total_size.saturating_add(entry.size);
                    if entry.size > 10 * 1024 * 1024
                        || total_size > MAX_SKILL_IMPORT_ARCHIVE_BYTES
                    {
                        return Err(CliError::with_code(
                            "skill_package_too_large",
                            "The GitHub Skill directory exceeds the import limit.",
                        ));
                    }
                    let download_url = entry.download_url.ok_or_else(|| {
                        CliError::with_code(
                            "skill_source_unavailable",
                            "GitHub did not provide a download URL for a Skill file.",
                        )
                    })?;
                    let expected_prefix = format!(
                        "https://raw.githubusercontent.com/{}/{}/",
                        github.owner, github.repo
                    );
                    if !download_url
                        .to_ascii_lowercase()
                        .starts_with(&expected_prefix.to_ascii_lowercase())
                    {
                        return Err(invalid_skill_import(
                            "GitHub returned an unexpected Skill file URL.",
                        ));
                    }
                    let bytes = download_remote_skill_file(&agent, &download_url)?;
                    total_size = total_size.saturating_sub(entry.size).saturating_add(bytes.len());
                    if bytes.len() > 10 * 1024 * 1024
                        || total_size > MAX_SKILL_IMPORT_ARCHIVE_BYTES
                    {
                        return Err(CliError::with_code(
                            "skill_package_too_large",
                            "The GitHub Skill directory exceeds the import limit.",
                        ));
                    }
                    let target = temporary.path().join(relative);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).map_err(to_cli_error)?;
                    }
                    fs::write(target, bytes).map_err(to_cli_error)?;
                }
                _ => {}
            }
        }
    }

    let archive_root = temporary.path().to_path_buf();
    let search_root = resolve_import_subpath(&archive_root, Some(subpath))?;
    Ok(PreparedRemoteArchive {
        _temporary: temporary,
        archive_root,
        search_root,
        source,
    })
}

fn github_contents_api_url(source: &GithubSkillSource, subpath: &str) -> String {
    format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        source.owner,
        source.repo,
        percent_encode_url_component(subpath, true),
        percent_encode_url_component(&source.revision, false),
    )
}

fn percent_encode_url_component(value: &str, preserve_slashes: bool) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (preserve_slashes && byte == b'/')
        {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

fn download_remote_skill_file(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, CliError> {
    use std::io::Read;
    let response = agent
        .get(url)
        .set("User-Agent", "MyOpenPanels-Skill-Importer")
        .call()
        .map_err(|error| {
            CliError::with_code(
                "skill_source_unavailable",
                format!("Could not download a GitHub Skill file: {error}"),
            )
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(to_cli_error)?;
    Ok(bytes)
}
