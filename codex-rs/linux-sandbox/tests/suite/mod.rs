// Aggregates all former standalone integration tests as modules.
mod bundled_bwrap;
mod landlock;
mod managed_proxy;
#[path = "unix_socket_tests.rs"]
mod unix_socket;
