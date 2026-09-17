///|
priv struct StudyRng {
  mut state : UInt64
}

///|
fn StudyRng::choose(self : StudyRng, n : Int) -> Int {
  self.state = self.state * 6364136223846793005 + 1442695040888963407
  ((self.state >> 32) % n.to_uint64()).to_int()
}

///|
priv enum StudyNode {
  Observation(Double)
  Group(Array[StudyNode])
}

///|
fn StudyNode::mean(self : StudyNode, rng : StudyRng?) -> Double {
  match self {
    Observation(value) => value
    Group(children) => {
      let mut total = 0.0
      for i in 0..<children.length() {
        let index = match rng {
          Some(rng) => rng.choose(children.length())
          None => i
        }
        total += children[index].mean(rng)
      }
      total / children.length().to_double()
    }
  }
}

///|
fn study_tree(pairs : Array[Json], level : Int) -> StudyNode raise {
  if level == 2 {
    return Group(
      pairs.map(p => {
        Observation(
          @json.from_json(field(p, "step_difference_mbtx_minus_shell")),
        )
      }),
    )
  }
  let key = if level == 0 { "scenario" } else { "variant" }
  let buckets : Map[String, Array[Json]] = Map([])
  for pair in pairs {
    let id = field(pair, key).stringify()
    let items = buckets
      .get(id)
      .unwrap_or_else(() => {
        let items = []
        buckets[id] = items
        items
      })
    items.push(pair)
  }
  let keys = buckets.keys().to_array()
  keys.sort()
  Group(keys.map(key => study_tree(buckets.get(key).unwrap(), level + 1)))
}

///|
/// Families are fixed strata. Resample paired scenarios, their input variants,
/// and repetitions; a repetition is never promoted to an independent scenario.
fn study_estimate(pairs : Array[Json]) -> Json raise {
  let comparable = pairs.filter(p => {
    field(p, "step_difference_mbtx_minus_shell") != Json::null()
  })
  if comparable.is_empty() {
    return {
      "pairs": 0,
      "mean_step_difference": Json::null(),
      "confidence_interval": Json::null(),
      "interval_reason": "No complete successful pairs",
    }
  }
  let groups : Map[String, Array[Json]] = Map([])
  for pair in comparable {
    let key = field(pair, "family").stringify() + "/" + field(pair,"complexity").stringify()
    let items = groups
      .get(key)
      .unwrap_or_else(() => {
        let items = []
        groups[key] = items
        items
      })
    items.push(pair)
  }
  let keys = groups.keys().to_array()
  keys.sort()
  let strata = keys.map(key => study_tree(groups.get(key).unwrap(), 0))
  let mut mean = 0.0
  for node in strata {
    mean += node.mean(None)
  }
  mean /= strata.length().to_double()
  let rng : StudyRng = { state: 20260916, }
  let draws : Array[Double] = []
  for _ in 0..<2000 {
    let mut total = 0.0
    for node in strata {
      total += node.mean(Some(rng))
    }
    draws.push(total / strata.length().to_double())
  }
  draws.sort()
  let observed_inputs : Map[String, Bool] = Map([])
  for pair in comparable {
    observed_inputs[field(pair, "task_id").stringify()] = true
  }
  let enough = groups.values().all(samples=>{
    let inputs : Map[String,Bool] = Map([])
    for sample in samples { inputs[field(sample,"task_id").stringify()]=true }
    inputs.length()>=2
  })
  {
    "pairs": comparable.length().to_json(),
    "observed_input_variants": observed_inputs.length().to_json(),
    "mean_step_difference": mean.to_json(),
    "confidence_interval": if enough {
      [draws[49].to_json(), draws[1949].to_json()]
    } else {
      Json::null()
    },
    "confidence_level": 0.95,
    "bootstrap_draws": 2000,
    "bootstrap_seed": 20260916,
    "method": "Percentile bootstrap; fixed category and complexity strata, paired scenario/input/repetition resampling; equal weights at each level",
    "interval_reason": if enough {
      "Conditional on successful pairs and observed benchmark coverage; no universal generalization"
    } else {
      "At least two distinct input cases are required in every observed category/complexity stratum; repeated sampling of one case is insufficient"
    },
  }
}

///|
fn study_uncertainty(pairs : Array[Json]) -> Json raise {
  let result = match study_estimate(pairs) { Object(v) => v; _ => abort("estimate") }
  let ratios = pairs.map(p=>{
    let copy = match p { Object(v)=>v.copy(); _=>Map([]) }
    copy["step_difference_mbtx_minus_shell"] = field(p,"step_ratio_mbtx_over_shell")
    Json::object(copy)
  })
  let ratio = study_estimate(ratios)
  result["mean_step_ratio"] = field(ratio,"mean_step_difference")
  result["ratio_confidence_interval"] = field(ratio,"confidence_interval")
  result["ratio_definition"] = "MBTX accepted steps / Shell accepted steps; ratios below 1 favor MBTX, conditional on both successes"
  Json::object(result)
}

///|
fn study_strata(
  pairs : Array[Json],
  attempts : Array[Json],
  key : String,
  intervals~ : Bool = true,
) -> Json raise {
  let labels : Map[String, Bool] = Map([])
  for pair in pairs {
    labels[string_field(pair, key)] = true
  }
  let names = labels.keys().to_array()
  names.sort()
  let result : Array[Json] = []
  for name in names {
    let selected = pairs.filter(p => field(p, key) == name.to_json())
    let selected_ids = selected.map(p => field(p, "pair_id"))
    let observed = attempts.filter(a => {
      selected_ids.contains(field(a, "pair_id"))
    })
    let itt : Map[String, Json] = Map([])
    for arm in ["shell_tool", "mbtx_program"] {
      let members = observed.filter(a => field(a, "arm") == arm.to_json())
      itt[arm] = {
        "assigned": selected.length().to_json(),
        "captured": members.length().to_json(),
        "successes": members
        .filter(a => field(a, "status") == Json::string("success"))
        .length()
        .to_json(),
        "external_failures": members
        .filter(a => {
          field(a, "status") == Json::string("relay_error") ||
          field(a, "status") == Json::string("provider_error")
        })
        .length()
        .to_json(),
      }
    }
    let estimate : Map[String,Json] = @json.from_json(study_uncertainty(selected))
    if !intervals {
      estimate["confidence_interval"] = Json::null()
      estimate["ratio_confidence_interval"] = Json::null()
      estimate["interval_reason"] = "Independent pilot; descriptive estimates only, excluded from formal study"
    }
    result.push({
      "name": name.to_json(),
      "itt": itt.to_json(),
      "conditional": estimate.to_json(),
    })
  }
  result.to_json()
}
