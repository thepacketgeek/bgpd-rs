# BGPd

BGP service daemon built in Rust

Totally just a POC, mostly for my own amusement

![PCAP](examples/pcap.png)


## Features
- [x] Listen for Incoming BGP sessions 
- [x] Parse OPEN, save capabilities
- [x] Send OPEN with capabilities 
- [x] Receive and respond to Keepalives
- [x] Attempt connection to unestablished peers
- [ ] Process UPDATE messagess, parsing with capabilities
- [ ] Store received routes locally
- [ ] Advertise routes (specified somewhere?)
- [ ] API/CLI interface for viewing peer status, routes, etc.

# Development
I'm currently using [ExaBGP](https://github.com/Exa-Networks/exabgp) (Python) to act as my BGP peer for testing.
- Here's an [intro article](https://thepacketgeek.com/influence-routing-decisions-with-python-and-exabgp/) about installing & getting started with ExaBGP.

## Testing Env setup
For ExaBGP I have the following files:

**conf.ini**
```ini
process announce-routes {
    run /path/to/python3 /path/to/announce.py;
    encoder json;
}

neighbor 127.0.0.1 {
# neighbor ::1 {   # Can use IPv6 also
    # local-address ::2;
    local-address 127.0.0.2;
    router-id 2.2.2.2;
    local-as 65000;
    peer-as 65000;
    # passive true;  // Uncomment to test active connections from bgpd

    capability {
        asn4 enable;
    }
    family {
        ipv4 unicast;
        ipv6 unicast;
    }

    api {
      processes [announce-routes];
    }
}
```

**announce.py**
```python
#!/usr/bin/env python3

from sys import stdout
from time import sleep

messages = [
    'announce route 100.10.0.0/24 next-hop self',
]

# Wait for session to come up
sleep(5)

for message in messages:
    stdout.write(f"{message}\n")
    stdout.flush()
    sleep(1)

#Loop endlessly to allow ExaBGP to continue running
while True:
    sleep(300)
```

Running the exabgp service with the command:

```
$ env exabgp.tcp.port=1179 exabgp.tcp.bind="127.0.0.2" exabgp ./conf.ini --once
```
> *--once only attempts a single connection, auto-quits when session ends*


And then running `bgpd` as follows:

Using IPv6
```
$ cargo run --  -a "::1" -p 1179 ./examples/config.toml -vv
```

or IPv4 (defaults to 127.0.0.1)
```
$ cargo run -- -p 1179 ./examples/config.toml -vv
```