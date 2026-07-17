use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;

const RANDOM_ID_BYTES: usize = 12;

pub fn random_base64url_96() -> String {
    let mut bytes = [0_u8; RANDOM_ID_BYTES];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn random_id(prefix: &str) -> String {
    format!("{prefix}:{}", random_base64url_96())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_suffix_is_sixteen_base64url_characters() {
        let suffix = random_base64url_96();
        assert_eq!(suffix.len(), 16);
        assert!(suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
    }

    #[test]
    fn random_id_preserves_prefix() {
        let id = random_id("task");
        assert!(id.starts_with("task:"));
        assert_eq!(id.len(), "task:".len() + 16);
    }
}
