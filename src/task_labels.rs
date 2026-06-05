pub(crate) fn normalize_task_label(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn task_label_index(labels: &[String], label: &str) -> Option<usize> {
    labels
        .iter()
        .position(|existing| existing.eq_ignore_ascii_case(label))
}

pub(crate) fn canonical_task_label(labels: &[String], label: &str) -> Option<String> {
    task_label_index(labels, label).map(|index| labels[index].clone())
}

#[cfg(test)]
mod tests {
    use super::{canonical_task_label, normalize_task_label, task_label_index};

    #[test]
    fn normalize_task_label_returns_none_for_empty_input() {
        assert_eq!(normalize_task_label(""), None);
    }

    #[test]
    fn normalize_task_label_returns_none_for_whitespace_only_input() {
        assert_eq!(normalize_task_label("   \t  "), None);
    }

    #[test]
    fn normalize_task_label_trims_and_returns_value() {
        assert_eq!(
            normalize_task_label("  Feature Work  "),
            Some("Feature Work".to_string())
        );
    }

    #[test]
    fn task_label_index_returns_index_for_exact_match() {
        let labels = vec!["Docs".to_string(), "Bugfix".to_string()];
        assert_eq!(task_label_index(&labels, "Docs"), Some(0));
    }

    #[test]
    fn task_label_index_returns_index_for_case_insensitive_match() {
        let labels = vec!["Label".to_string(), "Other".to_string()];
        assert_eq!(task_label_index(&labels, "label"), Some(0));
    }

    #[test]
    fn task_label_index_returns_none_for_missing_label() {
        let labels = vec!["Docs".to_string(), "Bugfix".to_string()];
        assert_eq!(task_label_index(&labels, "Planning"), None);
    }

    #[test]
    fn canonical_task_label_returns_canonical_stored_value_when_found() {
        let labels = vec!["Deep Work".to_string(), "Review".to_string()];
        assert_eq!(
            canonical_task_label(&labels, "deep work"),
            Some("Deep Work".to_string())
        );
    }

    #[test]
    fn canonical_task_label_returns_none_when_not_found() {
        let labels = vec!["Deep Work".to_string(), "Review".to_string()];
        assert_eq!(canonical_task_label(&labels, "Planning"), None);
    }
}
