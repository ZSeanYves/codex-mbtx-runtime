///|
test "interaction joins decision tool IDs without counting retries as steps" {
  let accounting : Json = {
    "steps": [
      {
        "step_id": "a",
        "emitted_tools": [
          { "observation": { "call_id": "read", "tool_name": "read_resource" } },
          { "observation": { "call_id": "bad", "tool_name": "mbtx" } },
        ],
      },
      {
        "step_id": "b",
        "emitted_tools": [
          { "observation": { "call_id": "page", "tool_name": "read_resource" } },
          { "observation": { "call_id": "ok", "tool_name": "mbtx" } },
        ],
      },
      {
        "step_id": "c",
        "emitted_tools": [
          {
            "observation": {
              "call_id": "unknown",
              "tool_name": "read_resource",
            },
          },
        ],
      },
    ],
  }
  let results : Json = {
    "read": { "arguments": { "resource_id": "reference:moonbit" } },
    "page": {
      "arguments": { "resource_id": "reference:moonbit", "offset": 123 },
    },
    "bad": {
      "output": {
        "status": "error",
        "stage": "compilation",
        "build": { "exit_code": 1 },
      },
    },
    "ok": {
      "output": {
        "status": "success",
        "stage": "completed",
        "build": { "exit_code": 0 },
      },
    },
  }
  let actual = interaction_statistics(accounting, results)
  assert_eq(field(actual, "first_successful_compile_step"), (2).to_json())
  assert_eq(field(actual, "reference_first_pages_observed"), (1).to_json())
  assert_eq(field(actual, "reference_continuations_observed"), (1).to_json())
  assert_eq(field(actual, "resource_reads_unknown"), (1).to_json())
  let expected : Json = [
    {
      "step": 1,
      "step_id": "a",
      "activities": ["reference_first_page", "compile_failure"],
    },
    {
      "step": 2,
      "step_id": "b",
      "activities": ["reference_continuation", "program_success"],
    },
    { "step": 3, "step_id": "c", "activities": ["resource_unknown"] },
  ]
  assert_eq(field(actual, "decisions"), expected)
  assert_eq(
    field(
      interaction_statistics({ "steps": [] }, results),
      "first_successful_compile_step",
    ),
    Json::null(),
  )
}
