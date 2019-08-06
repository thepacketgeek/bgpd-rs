# BGPd-rs

BGP service daemon built in Rust

Totally just a POC, mostly for my own amusement

![PCAP](examples/pcap.png)


## Features
- [x] Listen for Incoming BGP sessions 
- [x] Parse OPEN, save capabilities
- [x] Send OPEN with capabilities 
- [x] Receive and respond to Keepalives
- [x] Attempt connection to unestablished peers
- [x] Process UPDATE messages, parsing with capabilities
- [x] Store received routes locally
- [x] CLI interface for viewing peer status, routes, etc.
- [ ] Advertise routes (specified somewhere?)
- [ ] API/CLI interface for interacting with BGPd (advertising, refreshing peers, etc.)

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

```
$ curl -s http://127.0.0.1:8080/show/routes/learned | jq
[
  {
    "age": "00:00:02",
    "as_path": "",
    "communities": "404 65000.10",
    "local_pref": 100,
    "multi_exit_disc": 10,
    "next_hop": "127.0.0.2",
    "origin": "127.0.0.2",
    "prefix": "2.10.0.0",
    "received_from": "2.2.2.2"
  },
  {
    "age": "00:00:02",
    "as_path": "100 200",
    "communities": "",
    "local_pref": 100,
    "multi_exit_disc": null,
    "next_hop": "127.0.0.2",
    "origin": "127.0.0.2",
    "prefix": "2.200.0.0",
    "received_from": "2.2.2.2"
  }
]
```

## `bgpd-cli`
You can use the included for viewing peer & route information. [more info here](cli/README.md)

Current peer session status:
```
[~/bgpd-rs/] $ cargo run --bin cli -- show neighbors
Neighbor     AS     MsgRcvd  MsgSent  Uptime    State        PfxRcd
 ::0.0.0.2    65000  6        3        00:00:11  Established  0
 127.0.0.2    65000  0        0        ---       Idle         0
 172.16.20.1  65000  0        0        ---       Idle         0
 127.0.0.3    65000  0        0        ---       Idle         0
```

Learned routes:
```
[~/bgpd-rs/] $ cargo build
[~/bgpd-rs/] $ ./targets/debug/cli show routes learned
Neighbor  AFI   Prefix     Next Hop   Age       Origin  Local Pref  Metric  AS Path  Communities
 2.2.2.2   IPv4  2.10.0.0   127.0.0.2  00:00:10  IGP     100         10               404 65000.10
 2.2.2.2   IPv4  2.100.0.0  127.0.0.2  00:00:10  IGP     100         500              target:65000:1.1.1.1 redirect:65000:100
 2.2.2.2   IPv4  2.200.0.0  127.0.0.2  00:00:10  IGP     100                 100 200
 3.3.3.3   IPv4  3.100.0.0  127.0.0.3  00:00:09  IGP     100                 300
 3.3.3.3   IPv4  3.200.0.0  127.0.0.3  00:00:09  IGP     300
```
 > Tip: Use the `watch` command for keeping this view up-to-date



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
$ cargo run --bin bgpd --  -a "::1" -p 1179 ./examples/config.toml -vv
```

or IPv4 (defaults to 127.0.0.1)
```
$ cargo run --bin bgpd -- -p 1179 ./examples/config.toml -vv
```

You may notice that I'm using TCP port 1179 for testing, if you want/need to use TCP 179 for testing with a peer that can't change the port (*cough*Cisco*cough*), you need to run bgpd with sudo permissions:

```
$ cargo build
$ sudo ./targets/debug/bgpd ./examples/config.toml -vv
```