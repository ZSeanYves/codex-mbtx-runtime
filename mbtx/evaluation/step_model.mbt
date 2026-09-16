///|
priv struct StepEvent {
  seq : Int
  thread_id : Json
  codex_turn_id : Json
  observation : Json
} derive(FromJson)

///|
priv struct StepRecord {
  step_id : String
  thread_id : Json
  codex_turn_id : Json
  started_seq : Int
  mut ended_seq : Int?
  mut outcome : String?
  mut response_id : String?
  mut reason : String?
  inference_ids : Array[String]
  emitted_tools : Array[Json]
  dispatched_tools : Array[Json]
}

///|
priv struct RequestScope {
  scope_id : String
  inference_call_id : Json
  compaction_request_id : Json
  step_id : Json
  purpose : String
} derive(FromJson, ToJson)

///|
priv struct RequestRecord {
  request_id : String
  scope : RequestScope
  attempt : Int
  started_seq : Int
  mut ended_seq : Int?
  mut status_code : Int?
  mut error : String?
}

///|
priv struct InferenceLink {
  inference_call_id : String
  step_id : Json
  purpose : String
} derive(FromJson)

///|
fn field(value : Json, name : String) -> Json {
  match value {
    Object(fields) => fields.get(name).unwrap_or(Json::null())
    _ => Json::null()
  }
}

///|
fn string_field(value : Json, name : String) -> String raise {
  @json.from_json(field(value, name))
}

///|
fn object_field(value : Json, name : String) -> Map[String, Json] raise {
  @json.from_json(field(value, name))
}

///|
fn require_step(
  steps : Map[String, StepRecord],
  id : String,
) -> StepRecord raise {
  guard steps.get(id) is Some(step) else { fail("unknown step: \{id}") }
  step
}

///|
fn[T : @json.FromJson] nullable(value : Json) -> T? raise {
  match value {
    Null => None
    value => Some(@json.from_json(value))
  }
}

///|
fn[T : ToJson] nullable_json(value : T?) -> Json {
  match value {
    None => Json::null()
    Some(value) => value.to_json()
  }
}

///|
impl ToJson for StepRecord with fn to_json(self) {
  {
    "step_id": self.step_id.to_json(),
    "thread_id": self.thread_id,
    "codex_turn_id": self.codex_turn_id,
    "started_seq": self.started_seq.to_json(),
    "ended_seq": nullable_json(self.ended_seq),
    "outcome": nullable_json(self.outcome),
    "response_id": nullable_json(self.response_id),
    "reason": nullable_json(self.reason),
    "inference_ids": @json.to_json(self.inference_ids),
    "emitted_tools": @json.to_json(self.emitted_tools),
    "dispatched_tools": @json.to_json(self.dispatched_tools),
  }
}

///|
impl ToJson for RequestRecord with fn to_json(self) {
  {
    "request_id": self.request_id.to_json(),
    "scope": @json.to_json(self.scope),
    "attempt": self.attempt.to_json(),
    "started_seq": self.started_seq.to_json(),
    "ended_seq": nullable_json(self.ended_seq),
    "status_code": nullable_json(self.status_code),
    "error": nullable_json(self.error),
  }
}
