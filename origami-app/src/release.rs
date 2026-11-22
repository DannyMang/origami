use std::{env, process::Command};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const FEED: &str = "https://raw.githubusercontent.com/DannyMang/origami/main/latest.json";
const HOME: &str = "https://github.com/DannyMang/origami/";

pub type Update = (String, String, bool);

pub fn newer(tag: &str, have: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        let v = v.trim().trim_start_matches('v');
        let mut parts: Vec<u64> = v
            .split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        while parts.last() == Some(&0) {
            parts.pop();
        }
        parts
    };
    parts(tag) > parts(have)
}

pub fn read(feed: &[u8]) -> Option<Option<Update>> {
    let found: serde_json::Value = serde_json::from_slice(feed).ok()?;
    let (tag, url) = (found["version"].as_str()?.trim_start_matches('v'), found["url"].as_str()?);
    let word = |p: &str| !matches!(p, "." | "..") && p.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    let plain = |v: &str| v.len() <= 16 && v.chars().all(|c| c.is_ascii_digit() || c == '.');
    if !url.strip_prefix(HOME)?.split('/').all(word) || !plain(tag) {
        return None;
    }
    let least = found["minimum"].as_str().map(|m| m.trim_start_matches('v')).filter(|m| plain(m));
    let must = least.is_some_and(|m| newer(m, VERSION) && !newer(m, tag));
    Some(newer(tag, VERSION).then(|| (tag.to_string(), url.to_string(), must)))
}

pub fn latest() -> Option<Option<Update>> {
    if env::var_os("ORIGAMI_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    let out = origami_lang::output(Command::new("/usr/bin/curl").args([
        "-q",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "-fsL",
        "--max-time",
        "5",
        "--max-filesize",
        "65536",
        FEED,
    ]))
    .ok()?;
    read(&out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A release counts as an update only when its version is higher than the running one, trailing zeros
    // not counting.
    #[test]
    fn compares() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.1.1", "0.1.0"));
        assert!(newer("1.0", "0.9.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(newer("0.1.0.1", "0.1.0"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.1.0.0", "0.1.0"));
        assert!(!newer("0.1", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
        assert!(!newer("0.1.0-beta", "0.1.0"));
        assert!(!newer("junk", "0.1.0"));
    }

    // The feed names the newest version as plain numbers, a page of this project on GitHub to get it from
    // and, if it likes, the oldest version still supported; a feed with a lookalike or climbing address, a
    // version with words in it or no JSON at all reads as not checked rather than as up to date, and a
    // minimum that is not plain numbers or is above the newest version is ignored.
    #[test]
    fn reads() {
        let feed = |s: &str| read(s.as_bytes());
        let get = "https://github.com/DannyMang/origami/releases/latest";
        let up = feed(&format!(r#"{{ "version": "v99.0.0", "url": "{get}" }}"#));
        assert_eq!(up, Some(Some(("99.0.0".into(), get.into(), false))));
        let same = feed(&format!(r#"{{ "version": "{VERSION}", "minimum": "98.0.0", "url": "{get}" }}"#));
        assert_eq!(same, Some(None));
        let must = |least: &str| {
            let found = feed(&format!(r#"{{ "version": "99.0.0", "minimum": "{least}", "url": "{get}" }}"#));
            found.flatten().unwrap().2
        };
        assert!(must("98.0.0") && must("v99.0") && !must(VERSION) && !must("100.0.0") && !must("98.0.0 now"));
        for far in [
            "https://example.com/get",
            "https://github.com/DannyMang/origami.evil.com/",
            "https://github.com@evil.com/DannyMang/origami/",
            "https://github.com/DannyMang/origami/../../other/repo",
            "https://github.com/DannyMang/origami/%2e%2e/%2e%2e/other/repo",
            "https://github.com/DannyMang/origami/releases?to=https://evil.com",
        ] {
            assert_eq!(feed(&format!(r#"{{ "version": "99.0.0", "url": "{far}" }}"#)), None);
        }
        for odd in ["99.0.0 from evil.com", "99.0.0-beta", "9999999999999999.0"] {
            assert_eq!(feed(&format!(r#"{{ "version": "{odd}", "url": "{get}" }}"#)), None);
        }
        assert_eq!(
            feed(&format!(r#"{{ "version": "{VERSION}", "url": "https://example.com" }}"#)),
            None
        );
        assert_eq!(feed(r#"{ "version": "99.0.0", "url": "file:///etc/passwd" }"#), None);
        assert_eq!(feed(r#"{ "version": "99.0.0" }"#), None);
        assert_eq!(feed("404: Not Found"), None);
        assert_eq!(feed(""), None);
    }
}
