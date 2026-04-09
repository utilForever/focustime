use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "windows")]
const HOSTS_FILE: &str = r"C:\Windows\System32\drivers\etc\hosts";
#[cfg(not(target_os = "windows"))]
const HOSTS_FILE: &str = "/etc/hosts";
const BLOCK_MARKER_START: &str = "# focustime-block-start";
const BLOCK_MARKER_END: &str = "# focustime-block-end";

pub struct SiteBlocker {
    pub sites: Vec<String>,
    pub is_blocking: bool,
}

impl SiteBlocker {
    pub fn new() -> Self {
        Self {
            sites: Vec::new(),
            is_blocking: false,
        }
    }

    pub fn add_site(&mut self, site: String) {
        if let Some(hostname) = Self::sanitize_hostname(&site)
            && !self.sites.contains(&hostname)
        {
            self.sites.push(hostname);
        }
    }

    /// Validate and normalise a user-supplied hostname.
    /// Returns `None` if the input cannot be reduced to a valid single hostname.
    fn sanitize_hostname(input: &str) -> Option<String> {
        let mut hostname = input.trim().to_lowercase();

        if hostname.is_empty() {
            return None;
        }

        // Strip URI scheme (e.g. "https://example.com" → "example.com").
        if let Some(sep) = hostname.find("://") {
            hostname = hostname[sep + 3..].to_string();
        }

        // Remove path, query, or fragment after the hostname.
        if let Some(pos) = hostname.find(['/', '?', '#']) {
            hostname.truncate(pos);
        }

        if hostname.is_empty() {
            return None;
        }

        // Reject anything with internal whitespace (would produce multi-hostname lines).
        if hostname.chars().any(char::is_whitespace) {
            return None;
        }

        // Allow only ASCII letters, digits, dots, and hyphens.
        if !hostname
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        {
            return None;
        }

        Some(hostname)
    }

    pub fn remove_site(&mut self, index: usize) {
        if index < self.sites.len() {
            self.sites.remove(index);
        }
    }

    /// Activate blocking by writing entries into the hosts file.
    /// Returns an error if the file is not writable (e.g. needs sudo).
    pub fn block(&mut self) -> io::Result<()> {
        if self.sites.is_empty() {
            self.is_blocking = false;
            // Best-effort: strip any stale block section left by a prior run.
            let _ = self.remove_hosts_block();
            return Ok(());
        }
        self.apply_hosts_block()?;
        self.is_blocking = true;
        Ok(())
    }

    /// Remove the focustime block section from the hosts file.
    /// Always attempts to strip any existing block section, even after a crash
    /// left entries behind with is_blocking == false.
    pub fn unblock(&mut self) -> io::Result<()> {
        self.remove_hosts_block()?;
        self.is_blocking = false;
        Ok(())
    }

    /// Remove block entries on app exit (best-effort).
    pub fn cleanup(&mut self) {
        let _ = self.unblock();
    }

    fn apply_hosts_block(&self) -> io::Result<()> {
        let original = fs::read_to_string(HOSTS_FILE)?;
        // Detect the original line ending style so we don't convert CRLF → LF
        // on Windows hosts files.
        let nl = line_ending_for(&original);
        let mut content = Self::strip_block_section(&original);

        // Only insert a separator newline when the content doesn't already end
        // with one, so repeated focus/break cycles don't accumulate blank lines.
        if !content.ends_with(nl) && !content.is_empty() {
            content.push_str(nl);
        }
        content.push_str(BLOCK_MARKER_START);
        content.push_str(nl);
        for site in &self.sites {
            append_site_entries(&mut content, site, nl);
        }
        content.push_str(BLOCK_MARKER_END);
        content.push_str(nl);

        atomic_write_hosts(&content)?;
        flush_dns_cache();
        Ok(())
    }

    fn remove_hosts_block(&self) -> io::Result<()> {
        let content = fs::read_to_string(HOSTS_FILE)?;
        let cleaned = Self::strip_block_section(&content);
        // Only write back if something was actually removed.
        if cleaned != content {
            atomic_write_hosts(&cleaned)?;
            flush_dns_cache();
        }
        Ok(())
    }

    pub(crate) fn strip_block_section(content: &str) -> String {
        // Verify that both markers exist as *complete* lines in the correct order
        // before attempting to strip. This prevents a marker that appears only as
        // a substring of a longer line (e.g. "# focustime-block-end  extra") from
        // being found by a substring search but then silently missed by the line
        // loop, which would drop the remainder of the file.
        let has_block = {
            let mut found_start = false;
            content.lines().any(|line| {
                if !found_start {
                    found_start = line.trim() == BLOCK_MARKER_START;
                    false
                } else {
                    line.trim() == BLOCK_MARKER_END
                }
            })
        };
        if !has_block {
            return content.to_string();
        }

        // Preserve the original line ending style (LF vs CRLF).
        let nl = line_ending_for(content);
        let mut result = String::with_capacity(content.len());
        let mut in_block = false;

        for line in content.lines() {
            if line.trim() == BLOCK_MARKER_START {
                in_block = true;
                continue;
            }
            if line.trim() == BLOCK_MARKER_END {
                in_block = false;
                continue;
            }
            if !in_block {
                result.push_str(line);
                result.push_str(nl);
            }
        }

        result
    }
}

impl Default for SiteBlocker {
    fn default() -> Self {
        Self::new()
    }
}

fn line_ending_for(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn append_site_entries(content: &mut String, site: &str, nl: &str) {
    append_hosts_mapping(content, "127.0.0.1", site, nl);
    append_hosts_mapping(content, "::1", site, nl);
    if !site.starts_with("www.") {
        let www_site = format!("www.{site}");
        append_hosts_mapping(content, "127.0.0.1", &www_site, nl);
        append_hosts_mapping(content, "::1", &www_site, nl);
    }
}

fn append_hosts_mapping(content: &mut String, host: &str, site: &str, nl: &str) {
    content.push_str(host);
    content.push(' ');
    content.push_str(site);
    content.push_str(nl);
}

/// Write `content` to the hosts file atomically via a temp file + rename so
/// an interrupted write cannot corrupt the file or leave it truncated.
/// On non-Windows the original file's permissions are copied to the replacement.
/// On Windows we fall back to a direct write because atomic rename over an
/// existing file requires Win32 APIs not exposed by std::fs::rename.
fn atomic_write_hosts(content: &str) -> io::Result<()> {
    let hosts_path = Path::new(HOSTS_FILE);

    #[cfg(target_os = "windows")]
    {
        fs::write(hosts_path, content)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let dir = hosts_path.parent().unwrap_or(Path::new("."));
        let tmp_path = dir.join(".focustime_hosts.tmp");
        fs::write(&tmp_path, content)?;
        // Copy the original file's permissions onto the temp file so the rename
        // does not silently change the access mode of /etc/hosts.
        if let Ok(meta) = fs::metadata(hosts_path) {
            let _ = fs::set_permissions(&tmp_path, meta.permissions());
        }
        fs::rename(&tmp_path, hosts_path)
    }
}

/// Flush the OS DNS cache so /etc/hosts changes take effect immediately.
/// Best-effort: failures are silently ignored.
fn flush_dns_cache() {
    #[cfg(target_os = "macos")]
    {
        // Flush mDNSResponder cache (macOS 10.10.4+)
        let _ = Command::new("dscacheutil").arg("-flushcache").status();
        let _ = Command::new("killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
    }
    #[cfg(target_os = "linux")]
    {
        // systemd-resolved
        let _ = Command::new("systemd-resolve")
            .arg("--flush-caches")
            .status();
        // nscd (older systems)
        let _ = Command::new("nscd").args(["-i", "hosts"]).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("ipconfig").arg("/flushdns").status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_unterminated_start_marker_leaves_content_unchanged() {
        // A lone start marker without an end marker must not drop any content.
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_empty_string() {
        assert_eq!(SiteBlocker::strip_block_section(""), "");
    }

    #[test]
    fn strip_no_block_section_is_unchanged() {
        let input = "127.0.0.1 localhost\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_removes_block_section() {
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n::1 localhost\n";
        let expected = "127.0.0.1 localhost\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn strip_removes_block_section_at_end_of_file() {
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n";
        let expected = "127.0.0.1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn strip_multiple_sites_in_section() {
        let input = "before\n# focustime-block-start\n127.0.0.1 a.com\n127.0.0.1 b.com\n# focustime-block-end\nafter\n";
        let expected = "before\nafter\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn add_site_normalizes_and_deduplicates() {
        let mut b = SiteBlocker::new();
        b.add_site("  Example.COM  ".to_string());
        b.add_site("example.com".to_string());
        assert_eq!(b.sites, vec!["example.com"]);
    }

    #[test]
    fn add_site_ignores_empty() {
        let mut b = SiteBlocker::new();
        b.add_site("   ".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn add_site_strips_scheme_and_path() {
        let mut b = SiteBlocker::new();
        b.add_site("https://example.com/some/path?q=1".to_string());
        assert_eq!(b.sites, vec!["example.com"]);
    }

    #[test]
    fn add_site_rejects_multiple_hostnames() {
        let mut b = SiteBlocker::new();
        b.add_site("example.com other.com".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn add_site_rejects_invalid_characters() {
        let mut b = SiteBlocker::new();
        b.add_site("exam_ple.com".to_string());
        assert!(b.sites.is_empty());
    }

    #[test]
    fn strip_out_of_order_markers_leaves_content_unchanged() {
        // End marker before start marker: treat as corrupt, return unchanged.
        let input = "127.0.0.1 localhost\n# focustime-block-end\n# focustime-block-start\nafter\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_marker_with_trailing_content_leaves_unchanged() {
        // Markers that appear as substrings of longer lines must not be treated
        // as valid markers; the whole file should be returned untouched.
        let input = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end extra\n::1 localhost\n";
        assert_eq!(SiteBlocker::strip_block_section(input), input);
    }

    #[test]
    fn strip_preserves_crlf_line_endings() {
        let input = "127.0.0.1 localhost\r\n# focustime-block-start\r\n127.0.0.1 example.com\r\n# focustime-block-end\r\n::1 localhost\r\n";
        let expected = "127.0.0.1 localhost\r\n::1 localhost\r\n";
        assert_eq!(SiteBlocker::strip_block_section(input), expected);
    }

    #[test]
    fn remove_site_by_index() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        b.add_site("b.com".to_string());
        b.remove_site(0);
        assert_eq!(b.sites, vec!["b.com"]);
    }

    #[test]
    fn remove_site_out_of_bounds_is_safe() {
        let mut b = SiteBlocker::new();
        b.add_site("a.com".to_string());
        b.remove_site(5); // should not panic
        assert_eq!(b.sites.len(), 1);
    }
}
