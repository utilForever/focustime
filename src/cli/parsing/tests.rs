use super::*;

#[test]
fn parse_status_comparison_options_sets_fields_and_marks_usage() {
    let tokens = vec![
        ParsedToken::CompareBy(ComparisonDimension::Profile),
        ParsedToken::CompareTask(Some("Docs".to_string())),
        ParsedToken::CompareProfile(Some(ProfileBucket::Classic)),
        ParsedToken::CompareTimeOfDay(Some(TimeOfDayBucket::Night)),
        ParsedToken::CompareLimit(3),
    ];

    let (options, has_any) = parse_status_comparison_options(&tokens).unwrap();

    assert!(has_any);
    assert_eq!(options.dimension, ComparisonDimension::Profile);
    assert_eq!(options.task_label.as_deref(), Some("Docs"));
    assert_eq!(options.profile, Some(ProfileBucket::Classic));
    assert_eq!(options.time_of_day, Some(TimeOfDayBucket::Night));
    assert_eq!(options.limit, 3);
}

#[test]
fn parse_status_comparison_options_defaults_limit_when_omitted() {
    let tokens = vec![ParsedToken::CompareBy(ComparisonDimension::TaskLabel)];

    let (options, has_any) = parse_status_comparison_options(&tokens).unwrap();

    assert!(has_any);
    assert_eq!(options.limit, DEFAULT_STATUS_COMPARISON_LIMIT);
}

#[test]
fn parse_status_comparison_options_rejects_duplicate_compare_task() {
    let tokens = vec![
        ParsedToken::CompareTask(Some("Docs".to_string())),
        ParsedToken::CompareTask(Some("Build".to_string())),
    ];

    let error = parse_status_comparison_options(&tokens).unwrap_err();

    assert!(error.contains("`--compare-task` can only be specified once."));
}

#[test]
fn parse_status_comparison_options_rejects_duplicate_compare_time() {
    let tokens = vec![
        ParsedToken::CompareTimeOfDay(Some(TimeOfDayBucket::Morning)),
        ParsedToken::CompareTimeOfDay(Some(TimeOfDayBucket::Night)),
    ];

    let error = parse_status_comparison_options(&tokens).unwrap_err();

    assert!(error.contains("`--compare-time` can only be specified once."));
}

#[test]
fn parse_status_comparison_options_rejects_zero_limit() {
    let tokens = vec![ParsedToken::CompareLimit(0)];

    let error = parse_status_comparison_options(&tokens).unwrap_err();

    assert!(error.contains("`--compare-limit` requires a positive whole number."));
}

#[test]
fn parse_history_kpi_card_id_accepts_known_card() {
    let parsed = parse_history_kpi_card_id("focus_score").unwrap();

    assert_eq!(parsed, HistoryKpiCardId::FocusScore);
}

#[test]
fn parse_history_kpi_card_id_rejects_unknown_card() {
    let error = parse_history_kpi_card_id("legacy-card").unwrap_err();

    assert!(error.contains("Invalid history dashboard card"));
}

#[test]
fn parse_history_dashboard_order_value_accepts_complete_catalog() {
    let parsed = parse_history_dashboard_order_value(
        "focus_score,goal_streak,session_summary,focus_risk,weekly_allocation,last_interruption,stats_growth,retention,comparison_filters",
    )
    .unwrap();

    assert_eq!(parsed.len(), HistoryKpiCardId::all().len());
    assert_eq!(parsed[0], HistoryKpiCardId::FocusScore);
    assert_eq!(parsed[2], HistoryKpiCardId::SessionSummary);
}

#[test]
fn parse_history_dashboard_order_value_rejects_duplicates() {
    let error = parse_history_dashboard_order_value(
        "focus_score,focus_score,session_summary,goal_streak,focus_risk,weekly_allocation,last_interruption,stats_growth,retention,comparison_filters",
    )
    .unwrap_err();

    assert!(error.contains("Duplicate history dashboard card"));
}

#[test]
fn parse_history_dashboard_order_value_rejects_missing_cards() {
    let error = parse_history_dashboard_order_value(
        "focus_score,goal_streak,session_summary,focus_risk,weekly_allocation,last_interruption,stats_growth,retention",
    )
    .unwrap_err();

    assert!(error.contains("must include every KPI card exactly once"));
}
