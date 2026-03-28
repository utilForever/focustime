use std::{fs, io, path::PathBuf};

const BEGIN_MARKER: &str = "# BEGIN focustime";
const END_MARKER: &str = "# END focustime";

fn hosts_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// Injects redirect entries for `domains` into the hosts file, replacing any
/// previously injected block. Idempotent: calling it twice with the same list
/// results in exactly one block in the file.
pub fn apply_blocking(domains: &[String]) -> io::Result<()> {
    if domains.is_empty() {
        return remove_blocking();
    }

    let path = hosts_path();
    let original = fs::read_to_string(&path).unwrap_or_default();
    let stripped = strip_block(&original);

    let mut new_content = stripped;
    if !new_content.ends_with('\n') && !new_content.is_empty() {
        new_content.push('\n');
    }
    new_content.push_str(BEGIN_MARKER);
    new_content.push('\n');
    for domain in domains {
        let domain = domain.trim();
        if !domain.is_empty() {
            new_content.push_str(&format!("127.0.0.1 {domain}\n"));
        }
    }
    new_content.push_str(END_MARKER);
    new_content.push('\n');

    fs::write(&path, new_content)
}

/// Removes all lines previously injected by `apply_blocking`.
pub fn remove_blocking() -> io::Result<()> {
    let path = hosts_path();
    let original = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let stripped = strip_block(&original);
    if stripped == original {
        return Ok(()); // nothing to remove
    }

    fs::write(&path, stripped)
}

/// Returns the contents of `text` with the focustime marker block removed.
fn strip_block(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut inside = false;

    for line in text.lines() {
        if line.trim() == BEGIN_MARKER {
            inside = true;
            continue;
        }
        if line.trim() == END_MARKER {
            inside = false;
            continue;
        }
        if !inside {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_to(text: &str, domains: &[&str]) -> String {
        let domains: Vec<String> = domains.iter().map(|s| s.to_string()).collect();
        let stripped = strip_block(text);
        let mut new_content = stripped;
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str(BEGIN_MARKER);
        new_content.push('\n');
        for domain in &domains {
            new_content.push_str(&format!("127.0.0.1 {domain}\n"));
        }
        new_content.push_str(END_MARKER);
        new_content.push('\n');
        new_content
    }

    #[test]
    fn strip_removes_block() {
        let input = "127.0.0.1 localhost\n# BEGIN focustime\n127.0.0.1 bad.com\n# END focustime\n";
        let stripped = strip_block(input);
        assert_eq!(stripped, "127.0.0.1 localhost\n");
    }

    #[test]
    fn strip_is_noop_without_block() {
        let input = "127.0.0.1 localhost\n";
        assert_eq!(strip_block(input), input);
    }

    #[test]
    fn apply_injects_entries() {
        let base = "127.0.0.1 localhost\n";
        let result = apply_to(base, &["bad.com", "evil.com"]);
        assert!(result.contains("127.0.0.1 bad.com"));
        assert!(result.contains("127.0.0.1 evil.com"));
        assert!(result.contains(BEGIN_MARKER));
        assert!(result.contains(END_MARKER));
    }

    #[test]
    fn apply_replaces_existing_block() {
        let base = "127.0.0.1 localhost\n# BEGIN focustime\n127.0.0.1 old.com\n# END focustime\n";
        let result = apply_to(base, &["new.com"]);
        assert!(!result.contains("old.com"));
        assert!(result.contains("127.0.0.1 new.com"));
        // Only one BEGIN marker
        assert_eq!(result.matches(BEGIN_MARKER).count(), 1);
    }

    #[test]
    fn strip_handles_empty() {
        assert_eq!(strip_block(""), "");
    }
}
