use std::fs;
use std::io;
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
        let site = site.trim().to_lowercase();
        if !site.is_empty() && !self.sites.contains(&site) {
            self.sites.push(site);
        }
    }

    pub fn remove_site(&mut self, index: usize) {
        if index < self.sites.len() {
            self.sites.remove(index);
        }
    }

    /// Activate blocking by writing entries into /etc/hosts.
    /// Returns an error if the file is not writable (e.g. needs sudo).
    pub fn block(&mut self) -> io::Result<()> {
        if self.sites.is_empty() {
            self.is_blocking = false;
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
        let mut content = fs::read_to_string(HOSTS_FILE)?;
        content = Self::strip_block_section(&content);

        content.push('\n');
        content.push_str(BLOCK_MARKER_START);
        content.push('\n');
        for site in &self.sites {
            content.push_str(&format!("127.0.0.1 {site}\n"));
            content.push_str(&format!("::1 {site}\n"));
            if !site.starts_with("www.") {
                content.push_str(&format!("127.0.0.1 www.{site}\n"));
                content.push_str(&format!("::1 www.{site}\n"));
            }
        }
        content.push_str(BLOCK_MARKER_END);
        content.push('\n');

        fs::write(HOSTS_FILE, content)?;
        flush_dns_cache();
        Ok(())
    }

    fn remove_hosts_block(&self) -> io::Result<()> {
        let content = fs::read_to_string(HOSTS_FILE)?;
        let cleaned = Self::strip_block_section(&content);
        // Only write back if something was actually removed.
        if cleaned != content {
            fs::write(HOSTS_FILE, cleaned)?;
            flush_dns_cache();
        }
        Ok(())
    }

    pub(crate) fn strip_block_section(content: &str) -> String {
        // If either marker is absent the block section is incomplete/absent;
        // return the content unchanged to avoid silently dropping valid lines.
        if !content.contains(BLOCK_MARKER_START) || !content.contains(BLOCK_MARKER_END) {
            return content.to_string();
        }

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
                result.push('\n');
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
