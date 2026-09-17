//! Narrow host-configured IPC permission for a passive process observer.
//! The model cannot choose or extend this grant through a tool argument.
use codex_network_proxy::ManagedNetworkSandboxContext;
use codex_utils_absolute_path::AbsolutePathBuf;

pub(crate) fn with_observation_socket(
    context: Option<ManagedNetworkSandboxContext>,
    socket: Option<&AbsolutePathBuf>,
) -> Option<ManagedNetworkSandboxContext> {
    let Some(socket) = socket else { return context; };
    let mut context = context.unwrap_or(ManagedNetworkSandboxContext {
        loopback_ports: vec![], allow_local_binding: false,
        allow_unix_sockets: vec![], dangerously_allow_all_unix_sockets: false,
    });
    let path = socket.to_string_lossy().into_owned();
    if !context.allow_unix_sockets.contains(&path) { context.allow_unix_sockets.push(path); }
    Some(context)
}

#[cfg(test)]
#[path = "observed_process_tests.rs"]
mod tests;
