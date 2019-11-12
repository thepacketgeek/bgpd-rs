# BGPd CLI

This is an example CLI that uses the BGPd API to interact with a running instance of BGPd. It uses the default endpoint for the BGPd HTTP API (localhost:8080), but you can point to BGPd running remotely using the `--host` and `--port` options.

## Features
- [x] CLI interface for viewing peer status and details
- [x] View learned routes (with source)
- [x] View advertised routes
- [x] Advertise IPv4/IPv6 Unicast routes (More attribute support coming soon)
- [x] Advertise IPv4/IPv6 Flowspec flows
- [ ] Filter learned/advertised routes (prefix, peer, attributes, ...)
- [ ] Enable/disable Peers


# Show Commands

## Neighbors
Use `bgpd-cli` for viewing peer & route information:

Peer summary:
```
[~/bgpd-rs/examples/cli] $ cargo +nightly run -- show neighbors
 Neighbor     Router ID    AS     MsgRcvd  MsgSent  Uptime    State        PfxRcd
----------------------------------------------------------------------------------
 127.0.0.2    2.2.2.2      100    76       70       00:11:27  Established  4
 *127.0.0.3                65000                              Disabled
 172.16.20.2  172.16.20.2  65000  29       28       00:11:33  Established  2
```
 > Tip: Use the `watch` command for keeping this view up-to-date

Peer Detail:
```
BGP neighbor is 127.0.0.3,  remote AS 65000, local AS 65000
  *Peer is Disabled
  Neighbor capabilities:
    IPv4 Unicast
    IPv4 Flowspec
    IPv6 Unicast
    IPv6 Flowspec


BGP neighbor is 172.16.20.2,  remote AS 65000, local AS 65000
  BGP version 4,  remote router-id 172.16.20.2
    Local address: 172.16.20.90:55687
    Remote address: 172.16.20.2:179
  BGP state = Established, up for 00:11:59
  Hold time is 90 (00:01:18), keepalive interval is 30
    Last read 00:00:03, last write 00:00:11
  Neighbor capabilities:
    Address family IPv6 Unicast
    Address family IPv4 Unicast

  Message Statistics:
                      Sent      Received
    Total             30        29
```

## Routes

Learned routes:
```
[~/bgpd-rs/examples/cli] $ cargo +nightly build
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli show routes learned
IPv4 / Unicast
 Received From  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities           Age
--------------------------------------------------------------------------------------------------------------------------------
 Config         9.9.9.0/24      172.16.20.90  00:08:00  Incomplete                                                     00:08:00
 172.16.20.2    172.16.20.0/24  172.16.20.2   00:07:54  IGP         100                                                00:07:54
 127.0.0.2      2.100.0.0/24    127.0.0.2     00:07:46  IGP                     500     100      target:65000:1.1.1.1  00:07:46
 127.0.0.2      2.200.0.0/24    127.0.0.2     00:07:46  IGP                             100 200                        00:07:46

IPv6 / Flowspec
 Received From  Prefix                                          Next Hop  Age       Origin  Local Pref  Metric  AS Path  Communities     Age
--------------------------------------------------------------------------------------------------------------------------------------------------
 127.0.0.2      Dst: 3001:99:b::10/128, Src: 3001:99:a::10/128            00:00:39  IGP     100                          redirect:6:302  00:00:39

IPv6 / Unicast
 Received From  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
----------------------------------------------------------------------------------------------------------------------------------
 Config         3001:404:a::/64      3001:1::1           00:08:00  Incomplete                                            00:08:00
 Config         3001:404:b::/64      3001:1::1           00:08:00  Incomplete                                            00:08:00
 172.16.20.2    3001:172:16:20::/64  ::ffff:172.16.20.2  00:07:54  IGP         100                                       00:07:54
 127.0.0.2      2621:a:10::/64       3001:1::1           00:07:46  IGP                             600 650               00:07:46
 127.0.0.2      2621:a:1337::/64     3001:1::1           00:07:46  IGP                     404     100                   00:07:46
```

Advertised routes:
```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli show routes advertised
IPv4 / Unicast
 Advertised To  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
-----------------------------------------------------------------------------------------------------------------------
 127.0.0.2      172.16.20.0/24  172.16.20.2   00:08:01  IGP         100                                       00:08:01
 172.16.20.2    9.9.9.0/24      172.16.20.90  00:08:06  Incomplete                                            00:08:06

IPv6 / Unicast
 Advertised To  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
----------------------------------------------------------------------------------------------------------------------------------
 127.0.0.2      3001:172:16:20::/64  ::ffff:172.16.20.2  00:08:01  IGP         100                                       00:08:01
 172.16.20.2    3001:404:a::/64      3001:1::1           00:08:06  Incomplete                                            00:08:06
 172.16.20.2    3001:404:b::/64      3001:1::1           00:08:06  Incomplete                                            00:08:06
```

## Advertise

### Unicast
IPv4 Unicast
```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli advertise route 10.10.10.0/24 172.16.20.90 --local-pref 500
Added route to RIB for announcement:
 Received From  Prefix         Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
----------------------------------------------------------------------------------------------------------------------
 API            10.10.10.0/24  172.16.20.90  00:00:00  Incomplete                                            00:00:00
```

IPv6 Unicast
```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli advertise route 10.10.10.0/24 172.16.20.90 --local-pref 500
Added route to RIB for announcement:
 Received From  Prefix         Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
----------------------------------------------------------------------------------------------------------------------
 API            10.10.10.0/24  172.16.20.90  00:00:00  Incomplete                                            00:00:00
```

```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli show routes advertised
IPv4 / Unicast
 Advertised To  Prefix          Next Hop      Age       Origin      Local Pref  Metric  AS Path  Communities  Age
-----------------------------------------------------------------------------------------------------------------------
 ...
 172.16.20.2    10.10.10.0/24   172.16.20.90  00:01:17  Incomplete                                            00:01:17

IPv6 / Unicast
 Advertised To  Prefix               Next Hop            Age       Origin      Local Pref  Metric  AS Path  Communities  Age
----------------------------------------------------------------------------------------------------------------------------------
 ...
 172.16.20.2    3001:100:abcd::/64   3001:1::1           00:00:03  Incomplete                                            00:00:03
```

### Flowspec
```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli advertise flow ipv4 'traffic-rate 100' -m 'source 192.168.10.0/24'
Added flow to RIB for announcement:
 Received From  Prefix               Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities            Age
----------------------------------------------------------------------------------------------------------------------------------
 Config         Src 192.168.10.0/24            00:00:00  Incomplete                               traffic-rate:0:100bps  00:00:00
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli advertise flow ipv6 'redirect 100:200' -m 'destination 3001:10:20::/64'
Added flow to RIB for announcement:
 Received From  Prefix               Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities       Age
-----------------------------------------------------------------------------------------------------------------------------
 Config         Dst 3001:10:20::/64            00:00:00  Incomplete                               redirect:100:200  00:00:00
 ```

 ```
[~/bgpd-rs/examples/cli] $ ./targets/debug/bgpd-cli  show routes learned
IPv4 / Flowspec
 Received From  Prefix              Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities     Age
--------------------------------------------------------------------------------------------------------------------------
 Config         Src 192.168.0.0/16            00:01:55  Incomplete                               redirect:6:302  00:01:55

IPv6 / Flowspec
 Received From  Prefix                    Next Hop  Age       Origin      Local Pref  Metric  AS Path  Communities              Age
-----------------------------------------------------------------------------------------------------------------------------------------
 127.0.0.2      Dst 3001:99:b::10/128               00:01:24  IGP                             200      traffic-rate:0:500bps    00:01:24
                Src 3001:99:a::10/128
 Config         Src 3001:100::/56                   00:01:55  Incomplete                               traffic-rate:0:24000bps  00:01:55
                DstPort >8000, && <=8080
                Packet Length >100

```
