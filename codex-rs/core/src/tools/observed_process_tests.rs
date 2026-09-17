use super::*;
use pretty_assertions::assert_eq;

#[test]
fn explicit_socket_grant_does_not_enable_other_network_access() {
    assert_eq!(with_observation_socket(None, None), None);
    let socket = AbsolutePathBuf::from_absolute_path("/tmp/attempt/receipt.sock").unwrap();
    let expected = ManagedNetworkSandboxContext {
        loopback_ports: vec![],
        allow_local_binding: false,
        allow_unix_sockets: vec![socket.to_string_lossy().into_owned()],
        dangerously_allow_all_unix_sockets: false,
    };
    assert_eq!(
        with_observation_socket(None, Some(&socket)),
        Some(expected.clone())
    );
    assert_eq!(
        with_observation_socket(Some(expected.clone()), Some(&socket)),
        Some(expected)
    );
}
