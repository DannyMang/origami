use std::process::Command;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const FEED: &str = "https://raw.githubusercontent.com/DannyMang/origami/main/latest.json";

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
    (newer(tag, VERSION) && url.starts_with("https://")).then(|| (tag.to_string(), url.to_string()))
}

pub fn latest() -> Option<(String, String)> {
    let out = Command::new("curl").args(["-fsL", "--max-time", "5", FEED]).output().ok()?;
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

    // The feed names the newest version and where to get it; anything else means no update.
    #[test]
    fn reads() {
        let feed = |s: &str| read(s.as_bytes());
        let up = feed(r#"{ "version": "v99.0.0", "url": "https://example.com/get" }"#);
        assert_eq!(up, Some(("99.0.0".into(), "https://example.com/get".into())));
        assert_eq!(
            feed(&format!(r#"{{ "version": "{VERSION}", "url": "https://example.com" }}"#)),
            None
        );
        assert_eq!(feed(r#"{ "version": "99.0.0", "url": "file:///etc/passwd" }"#), None);
        assert_eq!(feed(r#"{ "version": "99.0.0" }"#), None);
        assert_eq!(feed("404: Not Found"), None);
    }
}
