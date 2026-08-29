#[path = "support/mod.rs"]
mod support;

use piko_protocol::{Command, CommandResult};
use support::{HostdHarness, serial_guard};

#[test]
fn configured_mcp_server_status_crosses_the_hostd_orchd_boundary() {
    let _serial = serial_guard();
    let mut host = HostdHarness::launch("mcp");

    host.send(Command::McpStatus {
        command_id: "mcp-status".into(),
    });
    let result = host.command_result("mcp-status");
    let CommandResult::McpStatusListed { servers, .. } = result else {
        panic!("expected MCP status list");
    };
    assert!(matches!(
        servers.as_slice(),
        [server]
            if server.name == "e2e-mcp"
                && !server.connected
                && server.error.is_some()
    ));
}
