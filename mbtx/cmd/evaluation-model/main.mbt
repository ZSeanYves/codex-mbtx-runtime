///|
async fn main {
  let args = @env.args()
  guard args.length() == 2 || args.length() == 3 else {
    fail("Usage: evaluation-model REDUCED_STATE.json [EXPECTED_COUNTS.json]")
  }
  let input = @json.parse(@fs.read_file(args[1]).text())
  let report = @evaluation.analyze_steps(input)
  if args.length() == 3 {
    let expected = @json.parse(@fs.read_file(args[2]).text())
    guard expected is { "observed": Object(counts), .. } else {
      fail("expected observed counts")
    }
    guard report is { "observed": Object(observed), .. } else {
      fail("missing report counts")
    }
    for name, count in counts {
      assert_eq(observed.get(name), Some(count))
    }
    guard report is { "complete": True, .. } else {
      fail(
        "fixed replay has incomplete accounting evidence: \{report.stringify()}",
      )
    }
  }
  println(report.stringify(indent=2))
}
