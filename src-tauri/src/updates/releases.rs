use semver::Version;
use serde::Deserialize;

pub const MAX_BYTES: usize = 1024 * 1024;
const DAY: u64 = 24 * 60 * 60;

#[derive(Deserialize)]
struct Release {
    draft: bool,
    tag_name: String,
}

pub fn newest(bytes: &[u8], current: &Version) -> Result<Option<Version>, super::Failure> {
    if bytes.len() > MAX_BYTES {
        return Err(super::Failure::TooLarge);
    }
    let releases: Vec<Release> =
        serde_json::from_slice(bytes).map_err(|_| super::Failure::Invalid)?;
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft && r.tag_name.len() <= 128)
        .filter_map(|r| Version::parse(r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name)).ok())
        .filter(|v| {
            (v.pre.is_empty() || !current.pre.is_empty()) && v.cmp_precedence(current).is_gt()
        })
        .max_by(Version::cmp_precedence))
}

pub fn due(enabled: Option<bool>, last: Option<u64>, now: u64) -> bool {
    enabled == Some(true) && last.is_none_or(|last| now.saturating_sub(last) >= DAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_by_semver_precedence_including_github_prereleases() {
        let bytes = br#"[{"draft":false,"tag_name":"v0.9.1","prerelease":true},
                        {"draft":false,"tag_name":"0.10.0"},{"draft":true,"tag_name":"v9.0.0"},
                        {"draft":false,"tag_name":"v0.11.0-rc.2"},{"draft":false,"tag_name":"bad"}]"#;
        for (current, expected) in [
            ("0.9.0", Some("0.10.0")),
            ("0.10.0+build", None),
            ("0.11.0", None),
            ("0.10.0-rc.1", Some("0.11.0-rc.2")),
        ] {
            assert_eq!(
                newest(bytes, &Version::parse(current).unwrap())
                    .unwrap()
                    .map(|v| v.to_string())
                    .as_deref(),
                expected
            );
        }
    }
    #[test]
    fn bounds_and_invalid_responses() {
        let v = Version::new(0, 1, 0);
        assert_eq!(newest(b"[]", &v), Ok(None));
        assert_eq!(newest(b"oops", &v), Err(super::super::Failure::Invalid));
        assert_eq!(
            newest(&vec![b' '; MAX_BYTES + 1], &v),
            Err(super::super::Failure::TooLarge)
        );
    }
    #[test]
    fn eligibility_includes_consent_and_handles_clock_rollback() {
        assert!(!due(None, None, 0));
        assert!(!due(Some(false), None, 0));
        assert!(due(Some(true), None, 0));
        assert!(!due(Some(true), Some(100), 99));
        assert!(!due(Some(true), Some(100), 100 + DAY - 1));
        assert!(due(Some(true), Some(100), 100 + DAY));
    }
}
