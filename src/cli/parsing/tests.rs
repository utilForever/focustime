use super::*;

#[test]
fn parse_history_kpi_card_id_accepts_known_card() {
    let parsed = parse_history_kpi_card_id("focus_score").unwrap();

    assert_eq!(parsed, HistoryKpiCardId::FocusScore);
}

#[test]
fn parse_history_kpi_card_id_accepts_trimmed_known_card() {
    let parsed = parse_history_kpi_card_id("  focus_score  ").unwrap();

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
