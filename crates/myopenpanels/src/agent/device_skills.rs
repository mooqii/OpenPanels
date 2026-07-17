#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSkillLocation {
    pub path: String,
    pub skill_path: String,
    pub scope: String,
    pub agents: Vec<String>,
    pub description: String,
    pub content_hash: String,
    pub comparison: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSkillGroup {
    pub key: String,
    pub name: String,
    pub description: String,
    pub locations: Vec<DeviceSkillLocation>,
    pub installed: Option<DeviceInstalledSkill>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInstalledSkill {
    pub id: String,
    pub kind: String,
    pub module_kinds: Vec<String>,
    pub content_hash: String,
    pub can_manage_associations: bool,
}

#[derive(Debug, Deserialize)]
struct DeviceSkillFrontmatter {
    name: String,
    description: String,
}

#[derive(Debug, Clone)]
struct DeviceSkillRoot {
    path: PathBuf,
    scope: &'static str,
    agent: &'static str,
}

#[derive(Debug)]
struct DiscoveredDeviceSkill {
    name: String,
    description: String,
    display_dir: PathBuf,
    scope: String,
    agents: BTreeSet<String>,
}

pub fn discover_device_skills(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let installed = installed_skills_by_identity(paths)?;
    let ignored = ignored_device_mismatches(paths)?;
    let discovered = discover_device_skill_instances(paths)?;
    let mut groups = BTreeMap::<String, DeviceSkillGroup>::new();
    for (canonical, skill) in discovered {
        let key = normalized_device_skill_name(&skill.name);
        let content_hash = skill_package_hash(&canonical)?;
        let installed_skill = installed.get(&key).cloned();
        let comparison = installed_skill
            .as_ref()
            .map(|item| {
                if item.content_hash == content_hash {
                    "same"
                } else if ignored.iter().any(|ignored| {
                    ignored["nameKey"].as_str() == Some(&key)
                        && ignored["path"].as_str() == Some(canonical.to_string_lossy().as_ref())
                        && ignored["installedHash"].as_str() == Some(&item.content_hash)
                        && ignored["deviceHash"].as_str() == Some(&content_hash)
                }) {
                    "ignored"
                } else {
                    "different"
                }
            })
            .unwrap_or("not-installed")
            .to_owned();
        let location = DeviceSkillLocation {
            path: skill.display_dir.display().to_string(),
            skill_path: skill.display_dir.join("SKILL.md").display().to_string(),
            scope: skill.scope,
            agents: skill.agents.into_iter().collect(),
            description: skill.description.clone(),
            content_hash,
            comparison,
        };
        let group = groups.entry(key.clone()).or_insert_with(|| DeviceSkillGroup {
            key: key.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            locations: Vec::new(),
            installed: installed_skill,
        });
        group.locations.push(location);
    }
    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.locations.sort_by(|left, right| left.path.cmp(&right.path));
    }
    groups.sort_by(|left, right| {
        left.installed
            .is_none()
            .cmp(&right.installed.is_none())
            .then_with(|| left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase()))
            .then_with(|| left.key.cmp(&right.key))
    });
    Ok(json!({
        "availableModuleKinds": ["wiki-update", "writing", "writing-refinement"],
        "skills": groups,
    }))
}

fn discover_device_skill_instances(
    paths: &MyOpenPanelsPaths,
) -> Result<BTreeMap<PathBuf, DiscoveredDeviceSkill>, CliError> {
    let mut discovered = BTreeMap::<PathBuf, DiscoveredDeviceSkill>::new();
    for root in device_skill_roots(paths)? {
        if !root.path.is_dir() {
            continue;
        }
        let mut visited = BTreeSet::new();
        scan_device_skill_root(&root, &root.path, 0, &mut visited, &mut discovered)?;
    }
    Ok(discovered)
}

fn installed_skills_by_identity(
    paths: &MyOpenPanelsPaths,
) -> Result<BTreeMap<String, DeviceInstalledSkill>, CliError> {
    let mut installed = BTreeMap::new();
    for listing in list_agent_skills(paths)? {
        let skill_path = PathBuf::from(&listing.local_dir).join("SKILL.md");
        let Some((identity_name, _)) = parse_device_skill(&skill_path)? else {
            continue;
        };
        let kind = managed_skill_kind(&listing)?.to_owned();
        installed.insert(
            normalized_device_skill_name(&identity_name),
            DeviceInstalledSkill {
                id: listing.skill.id.clone(),
                kind: kind.clone(),
                module_kinds: managed_skill_module_kinds(&listing),
                content_hash: skill_package_hash(Path::new(&listing.local_dir))?,
                can_manage_associations: kind == "custom",
            },
        );
    }
    Ok(installed)
}

fn ignored_device_mismatches(paths: &MyOpenPanelsPaths) -> Result<Vec<Value>, CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    Ok(storage
        .read_setting("skill_management", "ignored_device_mismatches")?
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default())
}

fn validated_device_skill(
    paths: &MyOpenPanelsPaths,
    location_path: &str,
) -> Result<(PathBuf, DiscoveredDeviceSkill), CliError> {
    let requested = fs::canonicalize(location_path).map_err(|_| {
        CliError::with_code("device_skill_not_found", "Device Skill directory is unavailable.")
    })?;
    let mut instances = discover_device_skill_instances(paths)?;
    instances
        .remove(&requested)
        .map(|skill| (requested, skill))
        .ok_or_else(|| {
            CliError::with_code(
                "device_skill_not_found",
                "Device Skill is no longer present in a known Skill directory.",
            )
        })
}

fn skill_package_hash(root: &Path) -> Result<String, CliError> {
    use sha2::Digest;
    let mut files = Vec::new();
    collect_skill_package_files(root, root, 0, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut digest = sha2::Sha256::new();
    for (relative, bytes) in files {
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn collect_skill_package_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), CliError> {
    if depth > 12 {
        return Err(CliError::with_code("skill_package_too_large", "Skill package has too many files or nested directories."));
    }
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_skill_package_files(root, &entry.path(), depth + 1, files)?;
            continue;
        }
        if !file_type.is_file() || (directory == root && entry.file_name() == "manifest.json") {
            continue;
        }
        if files.len() >= 512 {
            return Err(CliError::with_code("skill_package_too_large", "Skill package has too many files."));
        }
        let bytes = fs::read(entry.path()).map_err(to_cli_error)?;
        let total = files.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();
        if bytes.len() > 10 * 1024 * 1024 || total + bytes.len() > 50 * 1024 * 1024 {
            return Err(CliError::with_code("skill_package_too_large", "Skill package exceeds the import size limit."));
        }
        let relative = entry.path().strip_prefix(root).map_err(to_cli_error)?.to_string_lossy().replace('\\', "/");
        files.push((relative, bytes));
    }
    Ok(())
}

fn scan_device_skill_root(
    root: &DeviceSkillRoot,
    directory: &Path,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    discovered: &mut BTreeMap<PathBuf, DiscoveredDeviceSkill>,
) -> Result<(), CliError> {
    if depth > 5 {
        return Ok(());
    }
    let canonical = match fs::canonicalize(directory) {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    let skill_path = directory.join("SKILL.md");
    if skill_path.is_file() {
        if let Some((name, description)) = parse_device_skill(&skill_path)? {
            let entry = discovered
                .entry(canonical.clone())
                .or_insert_with(|| DiscoveredDeviceSkill {
                    name,
                    description,
                    display_dir: directory.to_path_buf(),
                    scope: root.scope.to_owned(),
                    agents: BTreeSet::new(),
                });
            entry.agents.insert(root.agent.to_owned());
            if root.scope == "project" {
                entry.scope = "project".to_owned();
            }
        }
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if matches!(name.to_str(), Some("node_modules" | ".git" | "dist" | "build" | "target" | "__pycache__")) {
            continue;
        }
        let path = entry.path();
        if entry
            .file_type()
            .ok()
            .is_some_and(|kind| kind.is_dir() || kind.is_symlink())
            && path.is_dir()
        {
            scan_device_skill_root(root, &path, depth + 1, visited, discovered)?;
        }
    }
    Ok(())
}

fn parse_device_skill(path: &Path) -> Result<Option<(String, String)>, CliError> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(_) => return Ok(None),
    };
    let normalized = source.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Ok(None);
    };
    let Some((frontmatter, _)) = rest.split_once("\n---") else {
        return Ok(None);
    };
    let metadata = match serde_yaml_ng::from_str::<DeviceSkillFrontmatter>(frontmatter) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    let name = metadata.name.trim();
    let description = metadata.description.trim();
    if name.is_empty() || description.is_empty() {
        return Ok(None);
    }
    Ok(Some((name.to_owned(), description.to_owned())))
}

fn normalized_device_skill_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn device_skill_roots(paths: &MyOpenPanelsPaths) -> Result<Vec<DeviceSkillRoot>, CliError> {
    let home = device_home_dir()?;
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let codex_home = env_path("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
    let claude_home = env_path("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home.join(".claude"));
    let autohand_home = env_path("AUTOHAND_HOME").unwrap_or_else(|| home.join(".autohand"));
    let hermes_home = env_path("HERMES_HOME").unwrap_or_else(|| home.join(".hermes"));
    let vibe_home = env_path("VIBE_HOME").unwrap_or_else(|| home.join(".vibe"));
    let mut roots = vec![
        global_root(home.join(".agents/skills"), "Universal"),
        global_root(home.join(".aider-desk/skills"), "AiderDesk"),
        global_root(home.join(".gemini/antigravity/skills"), "Antigravity"),
        global_root(
            home.join(".gemini/antigravity-cli/skills"),
            "Antigravity CLI",
        ),
        global_root(home.join(".astrbot/data/skills"), "AstrBot"),
        global_root(autohand_home.join("skills"), "Autohand Code"),
        global_root(home.join(".augment/skills"), "Augment"),
        global_root(home.join(".bob/skills"), "IBM Bob"),
        global_root(codex_home.join("skills"), "Codex"),
        global_root(claude_home.join("skills"), "Claude Code"),
        global_root(home.join(".cursor/skills"), "Cursor"),
        global_root(home.join(".codeartsdoer/skills"), "CodeArts Agent"),
        global_root(home.join(".codebuddy/skills"), "CodeBuddy"),
        global_root(home.join(".codemaker/skills"), "Codemaker"),
        global_root(home.join(".codestudio/skills"), "Code Studio"),
        global_root(home.join(".commandcode/skills"), "Command Code"),
        global_root(home.join(".gemini/skills"), "Gemini CLI"),
        global_root(home.join(".continue/skills"), "Continue"),
        global_root(home.join(".snowflake/cortex/skills"), "Cortex Code"),
        global_root(home.join(".deepagents/agent/skills"), "Deep Agents"),
        global_root(home.join(".factory/skills"), "Droid"),
        global_root(home.join(".firebender/skills"), "Firebender"),
        global_root(home.join(".forge/skills"), "ForgeCode"),
        global_root(home.join(".copilot/skills"), "GitHub Copilot"),
        global_root(hermes_home.join("skills"), "Hermes Agent"),
        global_root(home.join(".inferencesh/skills"), "Inference.sh"),
        global_root(home.join(".jazz/skills"), "Jazz"),
        global_root(home.join(".junie/skills"), "Junie"),
        global_root(home.join(".iflow/skills"), "iFlow CLI"),
        global_root(home.join(".kiro/skills"), "Kiro CLI"),
        global_root(home.join(".kilocode/skills"), "Kilo Code"),
        global_root(home.join(".kode/skills"), "Kode"),
        global_root(home.join(".lingma/skills"), "Lingma"),
        global_root(home.join(".mcpjam/skills"), "MCPJam"),
        global_root(vibe_home.join("skills"), "Mistral Vibe"),
        global_root(home.join(".moxby/skills"), "Moxby"),
        global_root(home.join(".mux/skills"), "Mux"),
        global_root(home.join(".openhands/skills"), "OpenHands"),
        global_root(home.join(".ona/skills"), "Ona"),
        global_root(home.join(".pi/agent/skills"), "Pi"),
        global_root(home.join(".qoder/skills"), "Qoder"),
        global_root(home.join(".qoder-cn/skills"), "Qoder CN"),
        global_root(home.join(".qwen/skills"), "Qwen Code"),
        global_root(home.join(".reasonix/skills"), "Reasonix"),
        global_root(home.join(".rovodev/skills"), "Rovo Dev"),
        global_root(home.join(".roo/skills"), "Roo Code"),
        global_root(home.join(".tabnine/agent/skills"), "Tabnine CLI"),
        global_root(home.join(".terramind/skills"), "Terramind"),
        global_root(home.join(".tinycloud/skills"), "TinyCloud"),
        global_root(home.join(".trae/skills"), "Trae"),
        global_root(home.join(".trae-cn/skills"), "Trae CN"),
        global_root(home.join(".codeium/windsurf/skills"), "Windsurf"),
        global_root(home.join(".zcode/skills"), "ZCode"),
        global_root(home.join(".zencoder/skills"), "Zencoder"),
        global_root(home.join(".neovate/skills"), "Neovate"),
        global_root(home.join(".pochi/skills"), "Pochi"),
        global_root(home.join(".adal/skills"), "Adal"),
        global_root(home.join(".openclaw/skills"), "OpenClaw"),
        global_root(config_home.join("opencode/skills"), "OpenCode"),
        global_root(config_home.join("agents/skills"), "Universal"),
        global_root(config_home.join("devin/skills"), "Devin"),
        global_root(config_home.join("goose/skills"), "Goose"),
        global_root(config_home.join("crush/skills"), "Crush"),
    ];
    roots.extend(workbuddy_skill_roots(&home));
    for (relative, agent) in [
        (".agents/skills", "Universal"),
        (".aider-desk/skills", "AiderDesk"),
        (".augment/skills", "Augment"),
        (".autohand/skills", "Autohand Code"),
        (".bob/skills", "IBM Bob"),
        (".claude/skills", "Claude Code"),
        (".codeartsdoer/skills", "CodeArts Agent"),
        (".codebuddy/skills", "CodeBuddy"),
        (".codemaker/skills", "Codemaker"),
        (".codestudio/skills", "Code Studio"),
        (".commandcode/skills", "Command Code"),
        (".codex/skills", "Codex"),
        (".continue/skills", "Continue"),
        (".cortex/skills", "Cortex Code"),
        (".crush/skills", "Crush"),
        (".cursor/skills", "Cursor"),
        (".devin/skills", "Devin"),
        (".factory/skills", "Droid"),
        (".forge/skills", "ForgeCode"),
        (".goose/skills", "Goose"),
        (".gemini/skills", "Gemini CLI"),
        (".hermes/skills", "Hermes Agent"),
        (".iflow/skills", "iFlow CLI"),
        (".inferencesh/skills", "Inference.sh"),
        (".jazz/skills", "Jazz"),
        (".junie/skills", "Junie"),
        (".kiro/skills", "Kiro CLI"),
        (".kilocode/skills", "Kilo Code"),
        (".kode/skills", "Kode"),
        (".lingma/skills", "Lingma"),
        (".mcpjam/skills", "MCPJam"),
        (".moxby/skills", "Moxby"),
        (".mux/skills", "Mux"),
        (".neovate/skills", "Neovate"),
        (".openhands/skills", "OpenHands"),
        (".ona/skills", "Ona"),
        (".pi/skills", "Pi"),
        (".pochi/skills", "Pochi"),
        (".qoder/skills", "Qoder"),
        (".qwen/skills", "Qwen Code"),
        (".reasonix/skills", "Reasonix"),
        (".rovodev/skills", "Rovo Dev"),
        (".roo/skills", "Roo Code"),
        (".tabnine/agent/skills", "Tabnine CLI"),
        (".terramind/skills", "Terramind"),
        (".tinycloud/skills", "TinyCloud"),
        (".trae/skills", "Trae"),
        (".vibe/skills", "Mistral Vibe"),
        (".windsurf/skills", "Windsurf"),
        (".zcode/skills", "ZCode"),
        (".zencoder/skills", "Zencoder"),
        ("agent/skills", "Eve"),
        ("data/skills", "AstrBot"),
        ("skills", "OpenClaw"),
    ] {
        roots.push(DeviceSkillRoot {
            path: paths.project_dir.join(relative),
            scope: "project",
            agent,
        });
    }
    Ok(roots)
}

fn global_root(path: PathBuf, agent: &'static str) -> DeviceSkillRoot {
    DeviceSkillRoot {
        path,
        scope: "global",
        agent,
    }
}

fn workbuddy_skill_roots(home: &Path) -> [DeviceSkillRoot; 2] {
    [
        global_root(home.join(".workbuddy/skills"), "WorkBuddy"),
        global_root(home.join(".workbuddy/connectors/skills"), "WorkBuddy"),
    ]
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn device_home_dir() -> Result<PathBuf, CliError> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| CliError::new("Could not resolve the user home directory."))
}
