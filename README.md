# BGPd-rs

BGP service daemon built in Rust
[![Actions Status](https://github.com/thepacketgeek/bgpd-rs/workflows/cargo/badge.svg)](https://github.com/thepacketgeek/bgpd-rs/actions)

Totally just a POC, mostly for my own amusement

![PCAP](examples/pcap.png)


## Features
- [x] Listen for Incoming BGP sessions 
- [x] Parse OPEN, save capabilities
- [x] Send OPEN with capabilities 
- [x] Receive and respond to Keepalives (on hold time based interval)
- [x] Process UPDATE messages, parsing with capabilities
- [x] Store received routes
- [x] CLI interface for viewing peer status, routes, etc.
- [x] Advertise routes to peers (specified from API)
- [x] API/CLI interface for interacting with BGPd
- [ ] Initiate outbound TCP connection to idle peers
- [ ] Route Policy for advertisement of learned routes (a.k.a Route Reflector)?

# Peer config
Peers and their config are defined in `TOML` format; see an example [here](examples/config.toml).

Details of config values:
```
router_id = "1.1.1.1"       # Default Router ID for the service
default_as = 65000          # Used as the local-as if `local_as` is not defined for a peer

[[peers]]
remote_ip = "127.0.0.2"     # This can also be an IPv6 address, see next peer
remote_as = 65000
passive = true              # If passive, bgpd won't attempt outbound connections
router_id = "127.0.0.1"     # Can override local Router ID for this peer
hold_timer = 90             # Set the hold timer for the peer, defaults to 180 seconds

[[peers]]
remote_ip = "::2"
remote_as = 65000
local_as = 100
```

# View BGPd Information
BGPd offers an HTTP API that can be queried to view operational info like neighbors and routes:

Neighbor uptime & prefixes received
```
$ curl -s http://127.0.0.1:8080/show/neighbors | jq '.[] | {peer: .neighbor, uptime: .uptime, prefixes: .prefixes_received}'
{
  "peer": "127.0.0.2",
  "uptime": "00:01:53",
  "pfxs": 3
}
{
  "peer": "127.0.0.3",
  "uptime": "00:01:43",
  "pfxs": 2
}
{
  "peer": "::0.0.0.2",
  "uptime": null,
  "pfxs": null
}
```

Learned routes (with attributes)
```
$ curl -s http://127.0.0.1:8080/show/routes/learned | jq
[
  {
    "age": "00:03:21",
    "as_path": "",
    "communities": "404 65000.10",
    "local_pref": 100,
    "multi_exit_disc": 10,
    "next_hop": "127.0.0.2",
    "origin": "127.0.0.2",
    "prefix": "2.10.0.0",
    "received_at": 1565200222,
    "received_from": "2.2.2.2"
  },
  {
    "age": "00:03:21",
    "as_path": "100 200",
    "communities": "",
    "local_pref": 100,
    "multi_exit_disc": null,
    "next_hop": "127.0.0.2",
    "origin": "127.0.0.2",
    "prefix": "2.200.0.0",
    "received_at": 1565200222,
    "received_from": "2.2.2.2"
  },
  ...
  {
    "age": "00:03:11",
    "as_path": "",
    "communities": "",
    "local_pref": 300,
    "multi_exit_disc": null,
    "next_hop": "127.0.0.3",
    "origin": "127.0.0.3",
    "prefix": "3.200.0.0",
    "received_at": 1565200232,
    "received_from": "3.3.3.3"
  }
]
```

Check out [bgpd-cli](examples/cli) for an example CLI you can use to view peer & route information via the BGPd API

# Advertising Routes
Adding routes to be advertised to certain peers can be done via the HTTP API:

```
curl http://127.0.0.1:8080/advertise/prefix/ -d '{"router_id": "2.2.2.2", "prefix": "9.9.9.0/24", "next_hop": "127.0.0.1"}' -H "Content-Type: application/json"
```

Will get advertised to the peer `2.2.2.2`:
```
$ bgpd-cli show routes advertised
 Neighbor  AFI   Prefix      Next Hop   Age       Origin      Local Pref  Metric  AS Path  Communities
-------------------------------------------------------------------------------------------------------
 2.2.2.2   IPv4  9.9.9.0/24  127.0.0.1  00:00:24  Incomplete
```

And showing up in the ExaBGP process
```
$ exabgpcli show adj-rib in
neighbor 127.0.0.1 ipv4 unicast 9.9.9.0/24 next-hop 127.0.0.1
```

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

```
$ env exabgp.tcp.port=1179 exabgp.tcp.bind="127.0.0.2" exabgp ./conf_127.0.0.2.ini --once
```
> *--once only attempts a single connection, auto-quits when session ends*


And then running `bgpd` as follows:

Using IPv6
```
$ cargo run -- -d -a "::1" -p 1179 ./examples/config.toml -vv
```

or IPv4 (defaults to 127.0.0.1)
```
$ cargo run -- -d -p 1179 ./examples/config.toml -vv
```

You may notice that I'm using TCP port 1179 for testing, if you want/need to use TCP 179 for testing with a peer that can't change the port (*cough*Cisco*cough*), you need to run bgpd with sudo permissions:

```
$ cargo build
$ sudo ./targets/debug/bgpd ./examples/config.toml -vv
```