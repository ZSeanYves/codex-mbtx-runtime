///|
fn report_assigned_arms(pair : Json) -> Array[String] {
  @json.from_json(field(pair, "assigned_arms")) catch {
    _ =>
      @json.from_json(field(pair, "arms")) catch {
        _ => ["shell_tool", "mbtx_program"]
      }
  }
}

///|
/// The collector already applies task filters and start_pair to schedule.
/// Exact slot selection leaves that schedule intact, so apply it only here.
fn report_schedule(
  manifest : Json,
  attempts : Array[Json],
) -> Array[Json] raise {
  let schedule : Array[Json] = @json.from_json(field(manifest, "schedule"))
  let slots : Array[String]? = nullable(
    field(field(manifest, "schedule_selection"), "slots"),
  )
  let known : Array[String] = []
  let result : Array[Json] = []
  for pair in schedule {
    let pair_id = string_field(pair, "pair_id")
    let arms = report_assigned_arms(pair).filter(arm => {
      let slot = "\{pair_id}/\{arm}"
      known.push(slot)
      match slots {
        Some(selected) => selected.contains(slot)
        None => true
      }
    })
    let copy : Map[String, Json] = @json.from_json(pair)
    copy["assigned_arms"] = arms.to_json()
    result.push(copy.to_json())
  }
  if slots is Some(selected) {
    let seen : Map[String, Bool] = Map([])
    for slot in selected {
      guard known.contains(slot) && !seen.contains(slot) else {
        fail("Report slot selection contains unknown or duplicate assignments")
      }
      seen[slot] = true
    }
  }
  let captured : Map[String, Bool] = Map([])
  for attempt in attempts {
    let pair_id = string_field(attempt, "pair_id")
    let arm = string_field(attempt, "arm")
    let slot = "\{pair_id}/\{arm}"
    guard result.any(pair => {
        string_field(pair, "pair_id") == pair_id &&
        report_assigned_arms(pair).contains(arm)
      }) &&
      !captured.contains(slot) else {
      fail(
        "Report attempt is outside the assigned scope or duplicates slot \{slot}",
      )
    }
    captured[slot] = true
  }
  result
}
