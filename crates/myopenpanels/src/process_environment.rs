use std::ffi::OsStr;
use std::process::Command;

const PROXY_ENVIRONMENT_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

pub(crate) fn remove_ephemeral_host_proxies(command: &mut Command) {
    if std::env::var_os("WORKBUDDY_PAC_RPC_SOCKET").is_none() {
        return;
    }
    for key in PROXY_ENVIRONMENT_KEYS {
        if std::env::var_os(key)
            .as_deref()
            .is_some_and(is_loopback_proxy)
        {
            command.env_remove(key);
        }
    }
}

fn is_loopback_proxy(value: &OsStr) -> bool {
    let Some(value) = value
        .to_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let authority = value
        .split_once("://")
        .map(|(_, authority)| authority)
        .unwrap_or(value)
        .split('/')
        .next()
        .unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map(|(_, authority)| authority)
        .unwrap_or(authority);
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed.split_once(']').map(|(host, _)| host)
    } else {
        authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .or(Some(authority))
    };
    host.is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost") || matches!(host, "127.0.0.1" | "::1")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_loopback_proxy_urls() {
        for value in [
            "http://127.0.0.1:64828",
            "https://localhost:8443",
            "socks5://user:pass@[::1]:1080",
            "127.0.0.1:8080",
        ] {
            assert!(is_loopback_proxy(OsStr::new(value)), "{value}");
        }
    }

    #[test]
    fn preserves_remote_and_lookalike_proxy_urls() {
        for value in [
            "http://proxy.example.com:8080",
            "http://localhost.example.com:8080",
            "http://127.0.0.2:8080",
            "",
        ] {
            assert!(!is_loopback_proxy(OsStr::new(value)), "{value}");
        }
    }
}
