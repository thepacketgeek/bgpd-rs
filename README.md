# BGPd-rs

BGP service daemon built in Rust
[![Actions Status](https://github.com/thepacketgeek/bgpd-rs/workflows/cargo/badge.svg)](https://github.com/thepacketgeek/bgpd-rs/actions)

![PCAP](examples/pcap.png)


## Features
- [x] Listen for Incoming BGP sessions 
- Specified peers can be an IP address or Network+Mask
- [x] Initiate outbound TCP connection to idle peers
- Will attempt connection based on configured poll interval
- [x] Negotiate OPEN Capabilities
- [x] Receive and respond to Keepalives (on hold time based interval)
- [x] Process UPDATE messages, store in RIB
- [x] Config reloading for Peer status (enable, passive, etc.)
  - [ ] Update static route advertisements mid-session
- [x] CLI interface for viewing peer status, routes, etc.
- [x] Advertise routes to peers (specified from API and/or Config) 
- [x] API/CLI interface for interacting with BGPd
- [x] Flowspec Support
- [ ] Route Refresh
- [ ] Neighbor MD5 Authentication
- [ ] Route Policy for filtering of learned & advertised routes

# Peer config
Peers and their config are defined in `TOML` format; see an example [here](examples/config.toml).

Details of config values:
```toml
router_id = "1.1.1.1"       # Default Router ID for the service
default_as = 65000          # Used as the local-as if `local_as` is not defined for a peer

[[peers]]
remote_ip = "127.0.0.2"     # This can also be an IPv6 address, see next peer
# remote_ip = "10.0.0.0/24"  # Network+Mask will accept inbound connections from any source in the subnet
remote_as = 65000
passive = true              # If passive, bgpd won't attempt outbound connections
router_id = "127.0.0.1"     # Can override local Router ID for this peer
hold_timer = 90             # Set the hold timer for the peer, defaults to 180 seconds
families = [                # Define the families this session should support
  "ipv4 unicast",
  "ipv6 unicast",
]
[[peers.static_routes]]     # Add static routes (advertised at session start)
  prefix = "9.9.9.0/24"
  next_hop = "127.0.0.1"
[[peers.static_routes]]
  prefix = "3001:100::/64"
  next_hop = "3001:1::1"
[[peers.static_flows]]     # Add static Flowspec rules too!
afi = 2
action = "traffic-rate 24000"
matches= [
    "source 3001:100::/56",
    "destination-port >8000 <=8080",
    "packet-length >100",
]
as_path = ["65000", "500"]
communities = ["101", "202", "65000:99"]


[[peers]]
remote_ip = "::2"
enabled = false             # Peer is essentially de-configured
remote_as = 100
local_as = 200
families = [
  "ipv6 unicast",
]
```

You can send the BGPd process a `SIGHUP` [E.g. `pkill -HUP bgpd$`] to reload and update peer configs. The following items can be updated:

## Peers
- Added & removed
- Enabled/disabled
- Active/passive polling for idle peers
- *Hold Timer
- *Supported Families

 > * When not in an active session only, since these are negotiated in the OPEN


# View BGPd Information
BGPd offers an JSON RCP API that can be queried to view operational info like neighbors and routes:

Neighbor uptime & prefixes received
```sh
$ curl localhost:8080 -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"show_peers","params":null,"id":0}' | jq '.result[] | {peer: .peer, uptime: .uptime, prefixes_received: .prefixes_received}'
{
  "peer": "127.0.0.2",
  "uptime": "00:31:13",
  "prefixes_received": 4
}
{
  "peer": "127.0.0.3",
  "uptime": null,
  "prefixes_received": null
}
{
  "peer": "172.16.20.2",
  "uptime": "00:31:20",
  "prefixes_received": 2
}
```

Learned routes (with attributes)
```sh
$ curl localhost:8080 -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"show_routes_learned","params": {"from_peer": "172.16.20.2"},"id":0}' | jq '.result[]'
{
  "afi": "IPv6",
  "age": "00:00:38",
  "as_path": "",
  "communities": [],
  "local_pref": 100,
  "multi_exit_disc": null,
  "next_hop": "::ffff:172.16.20.2",
  "origin": "IGP",
  "prefix": "3001:172:16:20::/64",
  "received_at": 1572898659,
  "safi": "Unicast",
  "source": "172.16.20.2"
}
{
  "afi": "IPv4",
  "age": "00:00:38",
  "as_path": "",
  "communities": [],
  "local_pref": 100,
  "multi_exit_disc": null,
  "next_hop": "172.16.20.2",
  "origin": "IGP",
  "prefix": "172.16.20.0/24",
  "received_at": 1572898659,
  "safi": "Unicast",
  "source": "172.16.20.2"
}
```

The `bgpd` [CLI](src/cli/mod.rs) can also be used to view peer & route information via the BGPd API (and announce routes too!)

# Development
I'm currently using [ExaBGP](https://github.com/Exa-Networks/exabgp) (Python) to act as my BGP peer for testing.
- Here's an [intro article](https://thepacketgeek.com/influence-routing-decisions-with-python-and-exabgp/) about installing & getting started with ExaBGP.

## Testing Env setup
For ExaBGP I have the following files (in the examples/exabgp dir):

**conf_127.0.0.2.ini**
```ini
neighbor 127.0.0.1 {
    router-id 2.2.2.2;
    local-address 127.0.0.2;          # Our local update-source
    local-as 65000;                    # Our local AS
    peer-as 65000;                    # Peer's AS

    announce {
        ipv4 {
            unicast 2.100.0.0/24 next-hop self med 500 extended-community [ target:65000:1.1.1.1 ];
            unicast 2.200.0.0/24 next-hop self as-path [ 100 200 ];
            unicast 2.10.0.0/24 next-hop self med 10 community [404 65000:10];
        }
    }
}
```

Running the exabgp service with the command:

```sh
$ env exabgp.tcp.port=1179 exabgp.tcp.bind="127.0.0.2" exabgp ./conf_127.0.0.2.ini --once
```
> *--once only attempts a single connection, auto-quits when session ends*


And then running `bgpd` as follows:

Using IPv6
```sh
$ cargo run -- -d -a "::1" -p 1179 ./examples/config.toml -vv
```

or IPv4 (defaults to 127.0.0.1)
```sh
$ cargo run -- -d -p 1179 ./examples/config.toml -vv
```

You may notice that I'm using TCP port 1179 for testing, if you want/need to use TCP 179 for testing with a peer that can't change the port (*cough*Cisco*cough*), you need to run bgpd with sudo permissions:

```sh
$ cargo build --release
$ sudo ./targets/release/bgpd ./examples/config.toml -vv
```

# Thanks to
- [bgp-rs](https://github.com/DevQps/bgp-rs) for the BGP Message Parsing
- [tokio](https://tokio.rs/) for the Runtime
- [ParityTech](https://github.com/paritytech/jsonrpsee) for the JSON RPC API