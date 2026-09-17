///|
test "distinct fixed strata cannot substitute for within-stratum input coverage" {
  let pairs : Array[Json] = []
  for category in ["a","b"] {
    for _ in 0..<2 {
      pairs.push({"family":category.to_json(),"complexity":"standard","scenario":category.to_json(),"variant":0,"task_id":(category+"0").to_json(),"step_difference_mbtx_minus_shell":(-2).to_json(),"step_ratio_mbtx_over_shell":0.8})
    }
  }
  let insufficient=study_uncertainty(pairs)
  assert_eq(field(insufficient,"mean_step_difference"),(-2.0).to_json())
  assert_eq(field(insufficient,"confidence_interval"),Json::null())
  assert_eq(field(insufficient,"ratio_confidence_interval"),Json::null())
  for category in ["a","b"] {
    pairs.push({"family":category.to_json(),"complexity":"standard","scenario":category.to_json(),"variant":1,"task_id":(category+"1").to_json(),"step_difference_mbtx_minus_shell":(-1).to_json(),"step_ratio_mbtx_over_shell":0.9})
  }
  assert_true(field(study_uncertainty(pairs),"confidence_interval")!=Json::null())
}
