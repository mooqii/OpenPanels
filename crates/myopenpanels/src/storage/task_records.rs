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
