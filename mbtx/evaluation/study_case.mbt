///|
/// A deterministic input instance and an independently computed answer. Replay
/// scripts are private validation aids and never enter the model workspace.
priv struct StudyCase {
  goal : String
  schema : String
  files : Map[String, Json]
  expected : Json
  reference : String
}

///|
// String's Compare uses shortlex; an oracle promising code-point order must
// explicitly compare decoded characters, including supplementary characters.
fn codepoint_compare(a : String, b : String) -> Int {
  let left = a.iter().to_array()
  let right = b.iter().to_array()
  for i in 0..<Int::min(left.length(), right.length()) {
    let order = left[i].compare(right[i])
    if order != 0 {
      return order
    }
  }
  left.length().compare(right.length())
}
