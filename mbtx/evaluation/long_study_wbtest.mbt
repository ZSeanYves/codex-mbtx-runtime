///|
test "long oracles reject wrong summaries, lost artifacts and invented execution" {
  for index in 0..<10 {
    for extended in [false,true] {
      let case = long_case(index, extended, 3)
      let task = case.json()
      let files = case.instance.files.copy()
      for path, expected in case.outputs {
        files[path] = match expected { String(_) => expected; _ => expected.stringify().to_json() }
      }
      for path in case.absent { files.remove(path) }
      files["result.json"] = case.instance.expected.stringify().to_json()
      let events : Array[Json] = case.worker_events.map(e => {
        {"event":{"phase":"completed","action":field(e,"action"),"id":field(e,"id"),"exit_code":field(e,"exit_code")},"confidence":"observed"}
      })
      let snapshot : Json = {"files":files.to_json(),"errors":[],"worker_events":events.to_json()}
      assert_eq(field(task_oracle(task,snapshot),"success"), true.to_json())
      files["result.json"] = "{}"
      assert_eq(field(task_oracle(task,{"files":files.to_json(),"errors":[],"worker_events":events.to_json()}),"success"), false.to_json())
      files["result.json"] = case.instance.expected.stringify().to_json()
      if !case.worker_events.is_empty() {
        assert_eq(field(task_oracle(task,{"files":files.to_json(),"errors":[],"worker_events":[]}),"success"), false.to_json())
      }
      for path, _ in case.outputs {
        let original = files[path]
        files[path] = "truncated tail"
        assert_eq(field(task_oracle(task,{"files":files.to_json(),"errors":[],"worker_events":events.to_json()}),"success"), false.to_json())
        files[path] = original
      }
    }
  }
}
