// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use reqwest::Url;
use std::net::IpAddr;

pub(crate) fn is_safe_rss_item_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return false;
    }

    let host = match url.host_str() {
        Some(host) => host,
        None => return false,
    };
    if host.eq_ignore_ascii_case("localhost") {
        return false;
    }
    let normalized_host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = normalized_host.parse::<IpAddr>() {
        match ip {
            IpAddr::V4(v4) => {
                if v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_multicast()
                    || v4.is_broadcast()
                    || v4.is_documentation()
                    || v4.is_unspecified()
                {
                    return false;
                }
            }
            IpAddr::V6(v6) => {
                if v6.is_loopback()
                    || v6.is_multicast()
                    || v6.is_unspecified()
                    || v6.is_unique_local()
                    || v6.is_unicast_link_local()
                {
                    return false;
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::is_safe_rss_item_url;

    #[test]
    fn rss_item_url_guard_rejects_localhost_and_private_literal_ips() {
        assert!(!is_safe_rss_item_url("http://localhost/file.torrent"));
        assert!(!is_safe_rss_item_url("https://127.0.0.1/file.torrent"));
        assert!(!is_safe_rss_item_url("https://192.168.10.5/file.torrent"));
        assert!(!is_safe_rss_item_url("https://[::1]/file.torrent"));
    }

    #[test]
    fn rss_item_url_guard_accepts_public_http_hosts() {
        assert!(is_safe_rss_item_url("https://example.com/file.torrent"));
        assert!(is_safe_rss_item_url(
            "http://downloads.example.net/a.torrent"
        ));
    }
}
