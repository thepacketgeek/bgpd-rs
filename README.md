# BGPd

BGP service daemon built in Rust

Totally just a POC, mostly for my own amusement


## Features
- [x] Listen for Incoming BGP sessions 
- [x] Parse OPEN, save capabilities
- [x] Send OPEN with capabilities 
- [x] Receive and respond to Keepalives
- [ ] Process UPDATE messagess, parsing with capabilities
- [ ] Store received routes locally
- [ ] Attempt connection to unestablished peers
- [ ] Advertise routes (specified somewhere?)
- [ ] API/CLI interface for viewing peer status, routes, etc.

# Development
I'm currently using [ExaBGP](https://github.com/Exa-Networks/exabgp) (Python) to act as my BGP peer for testing.

## Testing Env setup
For ExaBGP I have the following files:

**conf.ini**
```
process announce-routes {
    run /path/to/python3 /path/to/announce.py;
    encoder json;
}

neighbor 127.0.0.1 {
    router-id 2.2.2.2;
    local-address 127.0.0.2;          # Our local update-source
    local-as 65000;                    # Our local AS
    peer-as 65000;                    # Peer's AS

    api {
      processes [announce-routes];
    }
}
```

**announce.py**
```
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
$ env exabgp.tcp.port=1179 exabgp conf.ini -1
```
> *-1 only attempts a single connection, auto-quits when session ends*


And then running `bgpd` as follows:

```
$ cargo run --  -p 1179 ./examples/config.toml -vv
```