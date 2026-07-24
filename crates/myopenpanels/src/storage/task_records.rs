pub struct WrittenAsset {
    pub resource_id: String,
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

pub(crate) struct PreparedAssetWrite {
    pub(crate) asset_ref: String,
    pub(crate) content_hash: String,
    pub(crate) content_version: i64,
    pub(crate) file_name: String,
    pub(crate) file_path: PathBuf,
    pub(crate) resource_id: String,
    pub(crate) size_bytes: i64,
}

impl PreparedAssetWrite {
    pub(crate) fn written_asset(&self) -> WrittenAsset {
        WrittenAsset {
            resource_id: self.resource_id.clone(),
            asset_ref: self.asset_ref.clone(),
            file_name: self.file_name.clone(),
            file_path: self.file_path.clone(),
        }
    }
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
