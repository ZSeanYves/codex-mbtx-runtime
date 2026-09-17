use super::*;
use pretty_assertions::assert_eq;

#[test]
fn every_block_balances_categories_and_complexity_without_outcome_selection() {
    let mut tasks = Vec::new();
    for family in 0..10 {
        for complexity in ["standard", "extended"] {
            for variant in 0..2 {
                tasks.push(json!({"id":format!("{family}/{complexity}/{variant}"),"family":family.to_string(),"complexity":complexity,"variant":variant,"cohort":if family<8 {"workflow"} else {"program-delivery"}}));
            }
        }
    }
    let plan = schedule(&tasks, 2, 20260916);
    assert_eq!(plan.len(), 80);
    assert_eq!(plan, schedule(&tasks, 2, 20260916));
    for pair in plan.windows(2) {
        assert_ne!(pair[0]["arms"][0], pair[1]["arms"][0]);
    }
    for block in plan.chunks(10) {
        assert_eq!(
            block
                .iter()
                .map(|p| p["family"].to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            10
        );
        assert_eq!(
            block
                .iter()
                .filter(|p| p["complexity"] == "standard")
                .count(),
            5
        );
        assert_eq!(
            block
                .iter()
                .filter(|p| p["arms"][0] == "shell_tool")
                .count(),
            5
        );
    }
}
