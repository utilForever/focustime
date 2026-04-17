pub fn normalize_task_label(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn task_label_index(labels: &[String], label: &str) -> Option<usize> {
    labels
        .iter()
        .position(|existing| existing.eq_ignore_ascii_case(label))
}

pub fn canonical_task_label(labels: &[String], label: &str) -> Option<String> {
    task_label_index(labels, label).map(|index| labels[index].clone())
}
