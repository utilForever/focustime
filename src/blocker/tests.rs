use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_hosts_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("focustime-hosts-rollback-{label}-{unique}.tmp"))
}

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
fn add_site_strips_numeric_port() {
    let mut b = SiteBlocker::new();
    b.add_site("https://example.com:443/some/path".to_string());
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
fn add_site_accepts_wildcard_rules() {
    let mut b = SiteBlocker::new();
    b.add_site("*.Docs.Example.com".to_string());
    assert_eq!(b.sites, vec!["*.docs.example.com"]);
}

#[test]
fn add_site_rejects_mid_label_wildcard() {
    let mut b = SiteBlocker::new();
    b.add_site("foo*bar.example.com".to_string());
    assert!(b.sites.is_empty());
}

#[test]
fn bulk_add_accepts_comma_and_newline_separators() {
    let mut b = SiteBlocker::new();
    let result = b.add_sites_from_input("example.com, github.com\nhttps://rust-lang.org/docs");
    assert_eq!(
        result.added,
        vec!["example.com", "github.com", "rust-lang.org"]
    );
    assert!(result.duplicates.is_empty());
    assert!(result.invalid.is_empty());
    assert_eq!(b.sites, vec!["example.com", "github.com", "rust-lang.org"]);
}

#[test]
fn bulk_add_reports_duplicates_and_invalid_entries() {
    let mut b = SiteBlocker::new();
    let result = b.add_sites_from_input("github.com, bad host, exam_ple.com, github.com");
    assert_eq!(result.added, vec!["github.com"]);
    assert_eq!(result.duplicates, vec!["github.com"]);
    assert_eq!(
        result.invalid,
        vec![
            InvalidSiteInput {
                input: "bad host".to_string(),
                reason: SiteValidationError::ContainsWhitespace,
            },
            InvalidSiteInput {
                input: "exam_ple.com".to_string(),
                reason: SiteValidationError::InvalidCharacter,
            }
        ]
    );
}

#[test]
fn edit_site_updates_selected_entry() {
    let mut b = SiteBlocker::new();
    b.add_site("a.com".to_string());
    let result = b.edit_site_from_input(0, "https://news.ycombinator.com:443/newest");
    assert_eq!(
        result,
        EditSiteResult::Updated {
            old: "a.com".to_string(),
            new: "news.ycombinator.com".to_string()
        }
    );
    assert_eq!(b.sites, vec!["news.ycombinator.com"]);
}

#[test]
fn edit_site_rejects_duplicate_hostname() {
    let mut b = SiteBlocker::new();
    b.add_site("a.com".to_string());
    b.add_site("b.com".to_string());
    let result = b.edit_site_from_input(0, "b.com");
    assert_eq!(
        result,
        EditSiteResult::Duplicate {
            hostname: "b.com".to_string()
        }
    );
    assert_eq!(b.sites, vec!["a.com", "b.com"]);
}

#[test]
fn edit_site_rejects_multiple_hostnames() {
    let mut b = SiteBlocker::new();
    b.add_site("a.com".to_string());
    let result = b.edit_site_from_input(0, "a.com, b.com");
    assert_eq!(
        result,
        EditSiteResult::Invalid(InvalidSiteInput {
            input: "a.com, b.com".to_string(),
            reason: SiteValidationError::MultipleHostnames,
        })
    );
}

#[test]
fn wildcard_rule_matches_subdomains_only() {
    assert!(domain_rule_matches_host("*.example.com", "www.example.com"));
    assert!(domain_rule_matches_host("*.example.com", "a.b.example.com"));
    assert!(!domain_rule_matches_host("*.example.com", "example.com"));
}

#[test]
fn wildcard_rule_respects_label_boundaries() {
    assert!(!domain_rule_matches_host("*.example.com", "badexample.com"));
    assert!(!domain_rule_matches_host(
        "*.example.com",
        "example.com.bad"
    ));
}

#[test]
fn normalize_domain_rule_supports_wildcards_after_url_prefix_stripping() {
    assert_eq!(
        normalize_domain_rule("https://*.example.com/path").unwrap(),
        "*.example.com"
    );
}

#[test]
fn normalize_domain_rule_canonicalizes_dotted_forms() {
    assert_eq!(
        normalize_domain_rule(".Example.com").unwrap(),
        "*.example.com"
    );
    assert_eq!(
        normalize_domain_rule("example.com.").unwrap(),
        "example.com"
    );
    assert_eq!(
        normalize_domain_rule("https://.Example.com./path").unwrap(),
        "*.example.com"
    );
    assert_eq!(
        normalize_domain_rule(".com").unwrap_err(),
        SiteValidationError::InvalidLabel
    );
}

#[test]
fn normalize_domain_host_canonicalizes_root_dot_forms() {
    assert_eq!(
        normalize_domain_host(".WWW.Example.com.").unwrap(),
        "www.example.com"
    );
}

#[test]
fn wildcard_matching_handles_punycode_and_dotted_forms() {
    assert!(domain_rule_matches_host(
        ".xn--bcher-kva.example",
        "shop.xn--bcher-kva.example."
    ));
    assert!(domain_rule_matches_host(
        "*.example.com.",
        "service.example.com"
    ));
    assert!(!domain_rule_matches_host("*.example.com.", "example.com."));
}

#[test]
fn exact_rule_matches_only_exact_hostname() {
    assert!(domain_rule_matches_host("example.com", "example.com"));
    assert!(!domain_rule_matches_host("example.com", "www.example.com"));
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
    let removed = b.remove_site(0);
    assert_eq!(removed.as_deref(), Some("a.com"));
    assert_eq!(b.sites, vec!["b.com"]);
}

#[test]
fn remove_site_out_of_bounds_is_safe() {
    let mut b = SiteBlocker::new();
    b.add_site("a.com".to_string());
    assert!(b.remove_site(5).is_none()); // should not panic
    assert_eq!(b.sites.len(), 1);
}

#[test]
fn hosts_file_diagnostics_reports_read_and_write_success_for_temp_file() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("focustime-hosts-diagnostics-{unique}.tmp"));
    fs::write(&path, "127.0.0.1 localhost\n").expect("temp hosts file should be writable");

    let diagnostics = hosts_file_diagnostics_for(&path);

    assert!(diagnostics.can_read());
    assert!(diagnostics.can_write());
    assert!(diagnostics.read_error.is_none());
    assert!(diagnostics.write_error.is_none());
    let _ = fs::remove_file(path);
}

#[test]
fn hosts_file_diagnostics_reports_missing_file_errors() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("focustime-hosts-diagnostics-missing-{unique}.tmp"));
    let _ = fs::remove_file(&path);

    let diagnostics = hosts_file_diagnostics_for(&path);

    assert!(!diagnostics.can_read());
    assert!(diagnostics.read_error.is_some());
    #[cfg(target_os = "windows")]
    {
        assert!(!diagnostics.can_write());
        assert!(diagnostics.write_error.is_some());
    }
    #[cfg(not(target_os = "windows"))]
    {
        assert!(diagnostics.can_write());
        assert!(diagnostics.write_error.is_none());
    }
}

#[test]
fn preview_block_reports_next_section_and_change() {
    let mut blocker = SiteBlocker::new();
    blocker.add_site("example.com".to_string());
    let original = "127.0.0.1 localhost\n";

    let preview = blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Block);

    assert_eq!(preview.action, BlockingPreviewAction::Block);
    assert!(preview.would_change);
    assert_eq!(preview.current_section, None);
    assert_eq!(preview.effective_blocked_sites, vec!["example.com"]);
    let section = preview
        .next_section
        .as_deref()
        .expect("block preview should include next section");
    assert!(section.contains("# focustime-block-start"));
    assert!(section.contains("127.0.0.1 example.com"));
    assert!(section.contains("::1 www.example.com"));
    assert_eq!(preview.section_for_display(), Some(section));
}

#[test]
fn preview_block_skips_wildcard_entries_for_hosts_backend() {
    let mut blocker = SiteBlocker::new();
    blocker.add_site("*.example.com".to_string());
    blocker.add_site("api.example.com".to_string());
    let original = "127.0.0.1 localhost\n";

    let preview = blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Block);
    let section = preview
        .next_section
        .as_deref()
        .expect("block preview should include next section");

    assert_eq!(preview.effective_blocked_sites, vec!["api.example.com"]);
    assert!(!section.contains("*.example.com"));
    assert!(section.contains("127.0.0.1 api.example.com"));
}

#[test]
fn preview_unblock_reports_current_section_and_change() {
    let blocker = SiteBlocker::new();
    let original = "127.0.0.1 localhost\n# focustime-block-start\n127.0.0.1 example.com\n# focustime-block-end\n";

    let preview = blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Unblock);

    assert_eq!(preview.action, BlockingPreviewAction::Unblock);
    assert!(preview.would_change);
    assert!(preview.next_section.is_none());
    let section = preview
        .current_section
        .as_deref()
        .expect("unblock preview should include current section");
    assert!(section.contains("# focustime-block-start"));
    assert!(section.contains("# focustime-block-end"));
    assert_eq!(preview.section_for_display(), Some(section));
}

#[test]
fn preview_unblock_reports_all_existing_sections() {
    let blocker = SiteBlocker::new();
    let original = concat!(
        "127.0.0.1 localhost\n",
        "# focustime-block-start\n",
        "127.0.0.1 example.com\n",
        "# focustime-block-end\n",
        "# focustime-block-start\n",
        "127.0.0.1 github.com\n",
        "# focustime-block-end\n",
    );

    let preview = blocker.preview_from_hosts_content("hosts", original, BlockingIntent::Unblock);
    let section = preview
        .current_section
        .as_deref()
        .expect("unblock preview should include all current sections");

    assert_eq!(section.matches("# focustime-block-start").count(), 2);
    assert!(section.contains("127.0.0.1 example.com"));
    assert!(section.contains("127.0.0.1 github.com"));
}

#[test]
fn preview_block_no_change_when_hosts_already_match() {
    let mut blocker = SiteBlocker::new();
    blocker.add_site("example.com".to_string());
    let original =
        blocker.build_blocked_hosts_content("127.0.0.1 localhost\n", &["example.com".to_string()]);

    let preview = blocker.preview_from_hosts_content("hosts", &original, BlockingIntent::Block);

    assert_eq!(preview.action, BlockingPreviewAction::NoChange);
    assert!(!preview.would_change);
    assert!(preview.next_section.is_some());
    assert!(preview.current_section.is_some());
}

#[test]
fn block_rolls_back_to_original_after_post_replace_failure() {
    let path = temp_hosts_path("block-post-replace");
    let original = "127.0.0.1 localhost\n::1 localhost\n";
    fs::write(&path, original).expect("temp hosts file should be writable");

    let mut blocker = SiteBlocker::new();
    blocker.add_site("example.com".to_string());
    set_test_hosts_write_fail_steps(&[HostsWriteFailStep::AfterReplace]);

    let result = blocker.apply_hosts_block_to_path(&path);
    set_test_hosts_write_fail_steps(&[]);

    assert!(result.is_err());
    let restored = fs::read_to_string(&path).expect("hosts file should remain readable");
    assert_eq!(restored, original);
    assert!(!restored.contains(BLOCK_MARKER_START));
    assert!(!restored.contains(BLOCK_MARKER_END));

    let _ = fs::remove_file(path);
}

#[test]
fn block_failure_before_replace_keeps_original_content() {
    let path = temp_hosts_path("block-stage-write");
    let original = "127.0.0.1 localhost\n::1 localhost\n";
    fs::write(&path, original).expect("temp hosts file should be writable");

    let mut blocker = SiteBlocker::new();
    blocker.add_site("example.com".to_string());
    set_test_hosts_write_fail_steps(&[HostsWriteFailStep::StageWrite]);

    let result = blocker.apply_hosts_block_to_path(&path);
    set_test_hosts_write_fail_steps(&[]);

    assert!(result.is_err());
    let content = fs::read_to_string(&path).expect("hosts file should remain readable");
    assert_eq!(content, original);
    assert!(!content.contains(BLOCK_MARKER_START));
    assert!(!content.contains(BLOCK_MARKER_END));

    let _ = fs::remove_file(path);
}

#[test]
fn unblock_rolls_back_to_complete_section_after_post_replace_failure() {
    let path = temp_hosts_path("unblock-post-replace");
    let original = concat!(
        "127.0.0.1 localhost\n",
        "# focustime-block-start\n",
        "127.0.0.1 example.com\n",
        "::1 example.com\n",
        "# focustime-block-end\n",
        "::1 localhost\n",
    );
    fs::write(&path, original).expect("temp hosts file should be writable");

    let blocker = SiteBlocker::new();
    set_test_hosts_write_fail_steps(&[HostsWriteFailStep::AfterReplace]);

    let result = blocker.remove_hosts_block_from_path(&path);
    set_test_hosts_write_fail_steps(&[]);

    assert!(result.is_err());
    let restored = fs::read_to_string(&path).expect("hosts file should remain readable");
    assert_eq!(restored, original);
    assert_eq!(restored.matches(BLOCK_MARKER_START).count(), 1);
    assert_eq!(restored.matches(BLOCK_MARKER_END).count(), 1);

    let _ = fs::remove_file(path);
}

#[test]
fn rollback_failure_is_reported_when_restore_step_fails() {
    let path = temp_hosts_path("rollback-restore-error");
    let original = "127.0.0.1 localhost\n::1 localhost\n";
    fs::write(&path, original).expect("temp hosts file should be writable");

    let mut blocker = SiteBlocker::new();
    blocker.add_site("example.com".to_string());
    set_test_hosts_write_fail_steps(&[
        HostsWriteFailStep::AfterReplace,
        HostsWriteFailStep::RollbackRestore,
    ]);

    let result = blocker.apply_hosts_block_to_path(&path);
    set_test_hosts_write_fail_steps(&[]);

    let error = result.expect_err("rollback should fail when restore step is injected to fail");
    assert!(error.to_string().contains("rollback failed"));
    let content = fs::read_to_string(&path).expect("hosts file should remain readable");
    assert!(content.contains(BLOCK_MARKER_START));
    assert!(content.contains(BLOCK_MARKER_END));

    let _ = fs::remove_file(path);
}
