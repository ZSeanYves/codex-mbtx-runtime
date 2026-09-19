///|
test "request intervals require a clock and preserve missing or reversed boundaries" {
  assert_eq(
    request_interval((10).to_json(), (1000010).to_json(), "clock-a"),
    1.0.to_json(),
  )
  assert_eq(
    request_interval((10).to_json(), (1000010).to_json(), Json::null()),
    Json::null(),
  )
  assert_eq(
    request_interval((10).to_json(), Json::null(), "clock-a"),
    Json::null(),
  )
  assert_eq(
    request_interval((10).to_json(), (9).to_json(), "clock-a"),
    Json::null(),
  )
}

///|
test "timing phases are stable across unordered tool-result insertion" {
  let a : Json = {
    "output": {
      "target": "wasm",
      "cache": "miss",
      "preparation_ms": 4,
      "build": { "duration_ms": 8 },
      "run": { "duration_ms": 2 },
    },
  }
  let b : Json = {
    "output": {
      "target": "wasm",
      "cache": "hit",
      "preparation_ms": 1,
      "build": { "duration_ms": 8 },
      "run": { "duration_ms": 3 },
    },
  }
  let forward : Map[String, Json] = Map([])
  forward["call-a"] = a
  forward["call-b"] = b
  let backward : Map[String, Json] = Map([])
  backward["call-b"] = b
  backward["call-a"] = a
  assert_eq(
    attempt_timing({ "exchanges": [], "tool_results": forward.to_json() }),
    attempt_timing({ "exchanges": [], "tool_results": backward.to_json() }),
  )
}
