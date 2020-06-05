use std::sync::Arc;

use bgp_rs::Capabilities;

use crate::api::rpc::{PeerDetail, PeerSummary};
use crate::config::PeerConfig;
use crate::session::{Session, SessionState};
use crate::utils::{format_time_as_elapsed, get_host_address};

pub fn peer_to_summary(
    config: Arc<PeerConfig>,
    session: Option<&Session>,
    prefixes_received: Option<u64>,
) -> PeerSummary {
    PeerSummary {
        peer: session.map(|s| s.addr.to_string()).unwrap_or_else(|| {
            if let Some(addr) = get_host_address(&config.remote_ip) {
                addr.to_string()
            } else {
                config.remote_ip.to_string()
            }
        }),
        enabled: config.enabled,
        router_id: session.map(|s| s.router_id),
        remote_asn: config.remote_as,
        local_asn: config.local_as,
        msg_received: session.map(|s| s.counts.received()),
        msg_sent: session.map(|s| s.counts.sent()),
        connect_time: session.map(|s| s.connect_time.timestamp()),
        uptime: session.map(|s| format_time_as_elapsed(s.connect_time)),
        state: session.map(|s| s.state.to_string()).unwrap_or_else(|| {
            if !config.enabled {
                "Disabled".to_string()
            } else if !config.passive {
                SessionState::Active.to_string()
            } else {
                SessionState::Idle.to_string()
            }
        }),
        prefixes_received,
    }
}

pub fn peer_to_detail(
    config: Arc<PeerConfig>,
    session: Option<&Session>,
    prefixes_received: Option<u64>,
) -> PeerDetail {
    let capabilities = session
        .map(|s| capabilities_export(&s.capabilities))
        .unwrap_or_else(|| config.families.iter().map(|f| f.to_string()).collect());
    PeerDetail {
        summary: peer_to_summary(config.clone(), session, prefixes_received),
        capabilities,
        hold_timer: session
            .map(|s| s.hold_timer.hold_timer)
            .unwrap_or(config.hold_timer),
        hold_timer_interval: session
            .map(|s| s.hold_timer.interval)
            .unwrap_or(config.hold_timer / 3),
        hold_time: session.map(|s| s.hold_timer.to_string()),
        last_received: session.map(|s| format_time_as_elapsed(s.hold_timer.last_received)),
        last_sent: session.map(|s| format_time_as_elapsed(s.hold_timer.last_sent)),
        tcp_connection: session.map(|s| {
            let socket = s.protocol.get_ref();
            (
                socket
                    .local_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "---".to_string()),
                socket
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "---".to_string()),
            )
        }),
    }
}

fn capabilities_export(capabilities: &Capabilities) -> Vec<String> {
    let mut caps: Vec<String> = vec![];
    for fam in &capabilities.MP_BGP_SUPPORT {
        caps.push(format!(
            "Address family {} {}",
            fam.0.to_string(),
            fam.1.to_string()
        ))
    }
    if capabilities.ROUTE_REFRESH_SUPPORT {
        caps.push("Route Refresh".to_string());
    }
    caps
}
