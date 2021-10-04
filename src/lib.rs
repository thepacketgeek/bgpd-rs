#![recursion_limit = "512"] // Needed for the select! macros

//! BGPd-rs is a BGP peering utility
//!
//! BGPd can be started via the CLI:
//! ```sh
//! $ bgpd run path/to/config.toml
//! ```
//! The config TOML file is where you specify global BGP attributes and configure
//! peer details. Learn more about the options in the [`config`](./config/index.html) module.
//!
//! You change specify TCP port (default=179) or address (default=localhost):
//! ```sh
//! $ bgpd run path/to/config.toml --port 1179 --address 2601:1179::1
//! ```
//!
//! The JSON RPC API server defaults to localhost:8080, but can also be specified:
//! - `--api-addr 2601:1179::1`
//! - `--api-port 80`
//!
//! View more detailed logging by setting log verbosity (additive)
//! - `-v` DEBUG
//! - `-vv` TRACE
//! - `-vvv` TRACE (including tokio logs)
//!
//! To update the daemon with an updated config while it's running, send a SIGHUP:
//! ```sh
//! pkill -1 bgpd$
//! ```
//! The following peer items will be updated:
//! - Peers added, removed, enabled, disabled
//! - Active/passive polling for idle peers
//! - *Hold Timer
//! - *Supported Families
//!
//!>  **When not in an active session only, since these are negotiated in the OPEN*

/// JSON RPC API
pub mod api;
/// BGPd CLI for interacting with a running BGPd process
pub mod cli;
/// TOML Config Manager
/// Peers and their config are defined in `TOML` format.
///
///
/// Details of config values:
/// ```toml
/// router_id = "1.1.1.1"        # Default Router ID for the service
/// default_as = 65000           # Used as the local-as if `local_as` is not defined for a peer
///
/// [[peers]]
/// remote_ip = "127.0.0.2"      # This can also be an IPv6 address, see next peer
/// # remote_ip = "10.0.0.0/24"  # Network+Mask will accept inbound connections from any source in the subnet
/// remote_as = 65000
/// passive = true               # If passive, bgpd won't attempt outbound connections
/// router_id = "127.0.0.1"      # Can override local Router ID for this peer
/// hold_timer = 90              # Set the hold timer for the peer, defaults to 180 seconds
/// families = [                 # Define the families this session should support
///   "ipv4 unicast",
///   "ipv6 unicast",
/// ]
///
/// [[peers.static_routes]]      # Add static routes (advertised at session start)
///   prefix = "9.9.9.0/24"
///   next_hop = "127.0.0.1"
/// [[peers.static_routes]]
///   prefix = "3001:100::/64"
///   next_hop = "3001:1::1"
/// [[peers.static_flows]]       # Add static Flowspec rules too!
/// afi = 2
/// action = "traffic-rate 24000"
/// matches= [
///     "source 3001:100::/56",
///     "destination-port >8000 <=8080",
///     "packet-length >100",
/// ]
/// as_path = ["65000", "500"]
/// communities = ["101", "202", "65000:99"]
///
///
/// [[peers]]
/// remote_ip = "::2"
/// enabled = false              # Peer is essentially de-configured
/// remote_as = 100
/// local_as = 200
/// families = [
///   "ipv6 unicast",
/// ]
/// ```
pub mod config;
/// BGPd TCP listener
pub mod handler;
/// BGP Route Store
pub mod rib;
/// BGP Session Manager & Utils
pub mod session;
/// Misc BGP Message & Peer processing utilities
pub mod utils;
