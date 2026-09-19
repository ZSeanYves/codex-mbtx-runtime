///|
priv struct LongCase {
  instance : StudyCase
  outputs : Map[String, Json]
  mutable_inputs : Array[String]
  absent : Array[String]
  worker_events : Array[Json]
}

///|
fn long_case(
  index : Int,
  extended : Bool,
  variant : Int,
  basic? : Bool = false,
) -> LongCase {
  match index {
    0 => long_impact(extended, variant, basic~)
    1 | 8 => long_reconciliation(extended, variant, basic~)
    2 => long_incident(extended, variant, basic~)
    3 => long_migration(extended, variant, basic~)
    4 => long_jobs(extended, variant, basic~)
    5 | 9 => long_recovery(extended, variant, basic~)
    6 => long_forensics(extended, variant, basic~)
    _ => long_release(extended, variant, basic~)
  }
}

///|
fn LongCase::json(self : LongCase) -> Json {
  {
    "files": self.instance.files.to_json(),
    "expected": self.instance.expected,
    "output": "result.json",
    "expected_outputs": self.outputs.to_json(),
    "mutable_inputs": self.mutable_inputs.to_json(),
    "absent_outputs": self.absent.to_json(),
    "worker_events": self.worker_events.to_json(),
  }
}

///|
fn long_plain(instance : StudyCase) -> LongCase {
  {
    instance,
    outputs: Map([]),
    mutable_inputs: [],
    absent: [],
    worker_events: [],
  }
}
