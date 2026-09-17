///|
/// Additional files and host-observed worker events are independently checked;
/// a correct summary cannot substitute for missing effects or execution.
fn long_oracle(task : Json, snapshot : Json) -> Json raise {
  let files = object_field(snapshot,"files")
  for path, expected in object_field(task,"expected_outputs") {
    let actual = match (expected, files.get(path)) {
      (String(_), Some(actual)) => actual
      (_, Some(String(text))) => @json.parse(text) catch { _ => Json::null() }
      _ => Json::null()
    }
    if actual != expected {
      return {"success":false,"reason":("Missing or incorrect required artifact: " + path).to_json(),"confidence":"observed"}
    }
  }
  let absent : Array[String] = @json.from_json(field(task,"absent_outputs"))
  for path in absent {
    if files.contains(path) { return {"success":false,"reason":("Stale artifact was not removed: " + path).to_json(),"confidence":"observed"} }
  }
  let required : Array[Json] = @json.from_json(field(task,"worker_events"))
  if !required.is_empty() {
    let records : Array[Json] = @json.from_json(field(snapshot,"worker_events")) catch { _ => [] }
    if records.any(r => field(r,"evidence_error") != Json::null()) {
      return {"success":Json::null(),"reason":"Worker observation failed","confidence":"unknown"}
    }
    let completed = records.filter(r=>field(field(r,"event"),"phase") == Json::string("completed") && field(r,"confidence") == Json::string("observed"))
    for event in required {
      if !completed.any(r=>{
        let actual = field(r,"event")
        field(actual,"action") == field(event,"action") && field(actual,"id") == field(event,"id") && field(actual,"exit_code") == field(event,"exit_code")
      }) { return {"success":false,"reason":"Required real worker execution was not observed","confidence":"observed"} }
    }
    if required.any(e=>field(e,"action") == Json::string("recover")) && completed.any(r=>{
      let event = field(r,"event")
      field(event,"action") == Json::string("recover") && field(event,"exit_code") == (0).to_json() && !required.any(e=>field(e,"id") == field(event,"id"))
    }) { return {"success":false,"reason":"An already valid record was unnecessarily re-executed","confidence":"observed"} }
  }
  {"success":true,"reason":"Independent final artifacts, authorized mutation, absence and real worker receipts passed","confidence":"observed"}
}
