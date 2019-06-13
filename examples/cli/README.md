# BGPd-cli

This is an example CLI that uses the BGPd API to interact with a running instance of BGPd. It uses the default endpoint for the BGPd HTTP API (localhost:8080), but you can point to BGPd running remotely using the `--host` and `--port` options.

## Features
- [x] CLI interface for viewing peer status, routes, etc.
- [ ] Advertise routes (specified somewhere?)


# Commands
Currently the CLI is a one-trick-pony and just shows some info from BGPd.

## Show
Use `bgpd-cli` for viewing peer & route information:

Current peer session status:
```
[~/bgpd-rs/examples/cli] $ cargo run -- show neighbors
Neighbor     AS     MsgRcvd  MsgSent  Uptime    State        PfxRcd
 ::0.0.0.2    65000  6        3        00:00:11  Established  0
 127.0.0.2    65000                              Idle         
 127.0.0.3    65000                              Idle         
```

Learned routes:
```
[~/bgpd-rs/examples/cli] $ cargo build
[~/bgpd-rs/examples/cli] $ ./targets/debug/cli show routes learned
Neighbor  AFI   Prefix     Next Hop   Age       Origin  Local Pref  Metric  AS Path  Communities
 2.2.2.2   IPv4  2.10.0.0   127.0.0.2  00:00:10  IGP     100         10               404 65000.10
 2.2.2.2   IPv4  2.100.0.0  127.0.0.2  00:00:10  IGP     100         500              target:65000:1.1.1.1 redirect:65000:100
 2.2.2.2   IPv4  2.200.0.0  127.0.0.2  00:00:10  IGP     100                 100 200
 3.3.3.3   IPv4  3.100.0.0  127.0.0.3  00:00:09  IGP     100                 300
 3.3.3.3   IPv4  3.200.0.0  127.0.0.3  00:00:09  IGP     300
```
 > Tip: Use the `watch` command for keeping this view up-to-date
