//! Prespecified paired schedule; success and elapsed time never select samples.
use serde_json::Value;
use serde_json::json;

pub(crate) fn schedule(tasks: &[Value], repeats: usize, seed: u64) -> Vec<Value> {
    let mut order: Vec<_> = (0..tasks.len()).collect();
    let mut state = seed;
    for i in (1..order.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        order.swap(i, (state % (i as u64 + 1)) as usize);
    }
    let long = tasks.iter().all(|t| t["cohort"].is_string());
    if long {
        let mut families = std::collections::BTreeMap::<String, Vec<usize>>::new();
        for &index in &order {
            families
                .entry(tasks[index]["family"].to_string())
                .or_default()
                .push(index);
        }
        // One task per category in each ten-pair block. Alternate complexity
        // orientation across categories, with variants shuffled inside tiers.
        let mut buckets = families.into_values().collect::<Vec<_>>();
        for (i, bucket) in buckets.iter_mut().enumerate() {
            bucket.sort_by_key(|&j| tasks[j]["complexity"].as_str().unwrap_or_default());
            if !bucket.is_empty() {
                let shift = (i % 2) * (bucket.len() / 2);
                bucket.rotate_left(shift);
            }
        }
        order.clear();
        for round in 0..buckets.iter().map(Vec::len).max().unwrap_or(0) {
            let mut category_order = (0..buckets.len()).collect::<Vec<_>>();
            for i in (1..category_order.len()).rev() {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                category_order.swap(i, (state % (i as u64 + 1)) as usize);
            }
            for i in category_order {
                if let Some(&index) = buckets[i].get(round) {
                    order.push(index);
                }
            }
        }
    }
    let mut pairs = Vec::new();
    for repeat in 0..repeats {
        for (rank, &index) in order.iter().enumerate() {
            let n = pairs.len();
            let v3 = tasks[index].get("process_allow").is_some();
            let orientation = if long && !v3 { n } else { rank + repeat };
            pairs.push(json!({"pair_id":format!("pair-{n:04}"),"task_id":tasks[index]["id"],"track":tasks[index]["track"],"family":tasks[index]["family"],"scenario":tasks[index]["scenario"],"complexity":tasks[index]["complexity"],"cohort":tasks[index]["cohort"],"variant":tasks[index]["variant"],"repeat":repeat,"arms":if orientation%2==0 { ["shell_tool","mbtx_program"] } else { ["mbtx_program","shell_tool"] }}));
        }
    }
    pairs
}

#[cfg(test)]
#[path = "scheduling_tests.rs"]
mod tests;
