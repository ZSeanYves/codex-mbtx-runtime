///|
test "request intervals require a clock and preserve missing or reversed boundaries" {
  assert_eq(request_interval((10).to_json(),(1000010).to_json(),"clock-a"), (1.0).to_json())
  assert_eq(request_interval((10).to_json(),(1000010).to_json(),Json::null()), Json::null())
  assert_eq(request_interval((10).to_json(),Json::null(),"clock-a"), Json::null())
  assert_eq(request_interval((10).to_json(),(9).to_json(),"clock-a"), Json::null())
}
