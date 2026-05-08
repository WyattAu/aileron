use super::super::AppState;

pub fn cmd_arp(state: &mut AppState, query: &str) -> Option<()> {
    if query == "arp-start" {
        if let Some(ref server) = state.arp_server
            && server.is_running()
        {
            state.status_message = format!(
                "ARP server already running on ws://{}:{}",
                server.host(),
                server.port(),
            );
            return Some(());
        }
        let config = crate::arp::ArpConfig {
            port: state.config.arp_port.unwrap_or(19743),
            token: state.config.arp_token.clone(),
            ..Default::default()
        };
        match crate::arp::ArpServer::new(config) {
            Ok((server, receiver)) => match server.start() {
                Ok(()) => {
                    state.status_message =
                        format!("ARP server started on ws://127.0.0.1:{}", server.port(),);
                    state.arp_server = Some(server);
                    state.arp_cmd_receiver = Some(std::sync::Mutex::new(receiver));
                }
                Err(e) => {
                    state.status_message = format!("ARP server start failed: {e}");
                }
            },
            Err(e) => {
                state.status_message = format!("ARP server creation failed: {e}");
            }
        }
        return Some(());
    }

    if query == "arp-stop" {
        if let Some(ref server) = state.arp_server {
            server.stop();
            state.status_message = "ARP server stopped".into();
        } else {
            state.status_message = "ARP server is not running".into();
        }
        return Some(());
    }

    if query == "arp-status" {
        match state.arp_server {
            Some(ref server) => {
                let state_str = if server.is_running() {
                    "running"
                } else {
                    "stopped"
                };
                state.status_message = format!(
                    "ARP server: {} on ws://127.0.0.1:{}",
                    state_str,
                    server.port(),
                );
            }
            None => {
                state.status_message = "ARP server: not created (use :arp-start)".into();
            }
        }
        return Some(());
    }

    if query == "arp-token" {
        let token = uuid::Uuid::new_v4().to_string().replace('-', "");
        state.status_message = format!("Generated ARP token: {token}");
        state.config.arp_token = Some(token);
        return Some(());
    }

    None
}
