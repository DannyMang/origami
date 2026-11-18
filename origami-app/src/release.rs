use std::{env, process::Command};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const FEED: &str = "https://raw.githubusercontent.com/DannyMang/origami/main/latest.json";
const HOME: &str = "https://github.com/DannyMang/origami/";

pub fn newer(tag: &str, have: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        let v = v.trim().trim_start_matches('v');
        v.split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(tag) > parts(have)
}

pub fn read(feed: &[u8]) -> Option<(String, String)> {
    let found: serde_json::Value = serde_json::from_slice(feed).ok()?;
    let (tag, url) = (found["version"].as_str()?.trim_start_matches('v'), found["url"].as_str()?);
    (newer(tag, VERSION) && url.starts_with(HOME)).then(|| (tag.to_string(), url.to_string()))
}

pub fn latest() -> Option<(String, String)> {
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
        FEED,
    ]))
    .ok()?;
    read(&out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A release counts as an update only when its version is higher than the running one.
    #[test]
    fn compares() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.1.1", "0.1.0"));
        assert!(newer("1.0", "0.9.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
        assert!(!newer("0.1.0-beta", "0.1.0"));
        assert!(!newer("junk", "0.1.0"));
    }

    // The feed names the newest version and a page of this project on GitHub to get it from; anything
    // else, a lookalike address included, means no update.
    #[test]
    fn reads() {
        let feed = |s: &str| read(s.as_bytes());
        let get = "https://github.com/DannyMang/origami/releases/latest";
        let up = feed(&format!(r#"{{ "version": "v99.0.0", "url": "{get}" }}"#));
        assert_eq!(up, Some(("99.0.0".into(), get.into())));
        for far in [
            "https://example.com/get",
            "https://github.com/DannyMang/origami.evil.com/",
            "https://github.com@evil.com/DannyMang/origami/",
        ] {
            assert_eq!(feed(&format!(r#"{{ "version": "99.0.0", "url": "{far}" }}"#)), None);
        }
        assert_eq!(
            feed(&format!(r#"{{ "version": "{VERSION}", "url": "https://example.com" }}"#)),
            None
        );
        assert_eq!(feed(r#"{ "version": "99.0.0", "url": "file:///etc/passwd" }"#), None);
        assert_eq!(feed(r#"{ "version": "99.0.0" }"#), None);
        assert_eq!(feed("404: Not Found"), None);
    }
}
