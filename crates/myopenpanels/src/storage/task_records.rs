pub struct WrittenAsset {
    pub resource_id: String,
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

fn sanitize_asset_path(value: &str) -> String {
    let parts = value
        .split('/')
        .map(sanitize_path_part)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "asset.png".to_owned()
    } else {
        parts.join("/")
    }
}

fn unique_file_name(assets_dir: &std::path::Path, requested_name: &str) -> String {
    let requested = sanitize_asset_path(requested_name);
    if !assets_dir.join(&requested).exists() {
        return requested;
    }
    let path = std::path::Path::new(&requested);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("asset");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 1.. {
        let candidate = format!("{stem}-{index}{extension}");
        if !assets_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}
