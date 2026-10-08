use semver::Version;

/// Extracts a semantic version from a release tag such as `v0.5.0`, `0.5.0` or
/// `artcraft-v0.41.0`.
pub fn parse_tag(tag: &str) -> Option<Version> {
    let tag = tag.trim();
    tag.char_indices()
        .filter(|&(i, c)| c.is_ascii_digit() && (i == 0 || matches!(tag.as_bytes()[i - 1], b'v' | b'V' | b'-' | b'_')))
        .find_map(|(i, _)| Version::parse(&tag[i..]).ok())
}

/// True when `latest` is strictly newer than `installed`. Unparseable versions compare as
/// "different means newer" so a weird tag never strands a user on an old build.
pub fn is_newer(latest: &str, installed: &str) -> bool {
    match (parse_tag(latest), parse_tag(installed)) {
        (Some(l), Some(i)) => l > i,
        _ => latest.trim_start_matches('v') != installed.trim_start_matches('v'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_tags() {
        assert_eq!(parse_tag("v0.5.0"), Some(Version::new(0, 5, 0)));
        assert_eq!(parse_tag("0.5.0"), Some(Version::new(0, 5, 0)));
        assert_eq!(parse_tag("artcraft-v0.41.0"), Some(Version::new(0, 41, 0)));
        assert_eq!(parse_tag("v1.2.3-beta.1").unwrap().pre.as_str(), "beta.1");
        assert_eq!(parse_tag("nightly"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(!is_newer("v0.5.0", "0.5.0"));
        assert!(!is_newer("v0.4.0", "0.5.0"));
        assert!(is_newer("nightly-2", "nightly-1"));
    }
}
