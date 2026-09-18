//! Exercise passive IPC through the same sandbox transform as native tools.
#![cfg(target_os = "linux")]

use codex_network_proxy::ManagedNetworkSandboxContext;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::SandboxCommand;
use codex_sandboxing::SandboxManager;
use codex_sandboxing::SandboxTransformRequest;
use codex_sandboxing::SandboxType;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;

#[test]
fn passive_unix_socket_grant_preserves_restricted_ip_network() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let cwd = AbsolutePathBuf::from_absolute_path(temporary.path().canonicalize()?)?;
    let socket = cwd.join("observer.sock");
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let helper = super::landlock::codex_linux_sandbox_exe();
    let script = r#"
import socket, sys
for family in [socket.AF_INET, socket.AF_INET6]:
    try:
        socket.socket(family, socket.SOCK_STREAM)
    except PermissionError:
        pass
    else:
        raise AssertionError("IP socket creation was permitted")
client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    client.connect(sys.argv[1])
except PermissionError:
    assert sys.argv[2] == "denied"
else:
    assert sys.argv[2] == "allowed"
    client.sendall(b"observed")
print(sys.argv[2])
"#;
    for grant in ["denied", "allowed"] {
        let context = (grant == "allowed").then(|| ManagedNetworkSandboxContext {
            allow_unix_sockets: vec![socket.to_string_lossy().into_owned()],
            ..Default::default()
        });
        let request = SandboxManager::new().transform(SandboxTransformRequest {
            command: SandboxCommand {
                program: "python3".into(),
                args: vec!["-c".into(), script.into(), socket.to_string_lossy().into_owned(), grant.into()],
                cwd: cwd.clone().into(),
                env: Default::default(),
                managed_network: context,
                additional_permissions: None,
            },
            permissions: &PermissionProfile::read_only(),
            sandbox: SandboxType::LinuxSeccomp,
            enforce_managed_network: false,
            environment_id: None,
            network: None,
            sandbox_policy_cwd: &cwd.clone().into(),
            sandbox_exe: Some(&helper),
            use_legacy_landlock: false,
            windows_sandbox_level: WindowsSandboxLevel::Disabled,
            windows_sandbox_private_desktop: false,
        })?;
        assert!(!request.command.iter().any(|arg| arg == "--managed-network"));
        let mut command = std::process::Command::new(&request.command[0]);
        command.args(&request.command[1..]).current_dir(cwd.as_path());
        if let Some(arg0) = request.arg0 { command.arg0(arg0); }
        let output = command.output()?;
        assert_eq!((output.status.code(), output.stdout, output.stderr), (Some(0), format!("{grant}\n").into_bytes(), vec![]));
    }
    let (mut peer, _) = listener.accept()?;
    let mut received = Vec::new();
    peer.read_to_end(&mut received)?;
    assert_eq!(received, b"observed");
    Ok(())
}
