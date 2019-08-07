use bgp_rs::Segment;
use futures::future;
use hyper::rt::Future;
use hyper::{Body, Request, Response};
use hyper::{Method, StatusCode};
use log::{error, trace};
use serde_json::{self, Map, Number, Value};

use crate::db::DB;
use crate::utils::format_time_as_elapsed;

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

pub fn handle_api_request(req: Request<Body>) -> BoxFut {
    let mut response = Response::new(Body::empty());
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/show/neighbors") => match DB::new().and_then(|db| db.get_all_peers()) {
            Ok(peers) => {
                let output: Vec<Value> = peers
                    .iter()
                    .map(|peer| {
                        let mut data: Map<String, Value> = Map::new();
                        data.insert(
                            "neighbor".to_string(),
                            Value::String(peer.neighbor.to_string()),
                        );
                        data.insert(
                            "router_id".to_string(),
                            match peer.router_id {
                                Some(value) => Value::String(value.to_string()),
                                None => Value::Null,
                            },
                        );
                        data.insert("asn".to_string(), Value::Number(Number::from(peer.asn)));
                        data.insert(
                            "msg_received".to_string(),
                            match peer.msg_received {
                                Some(value) => Value::Number(Number::from(value)),
                                None => Value::Null,
                            },
                        );
                        data.insert(
                            "msg_sent".to_string(),
                            match peer.msg_sent {
                                Some(value) => Value::Number(Number::from(value)),
                                None => Value::Null,
                            },
                        );
                        data.insert(
                            "connect_time".to_string(),
                            match peer.connect_time {
                                Some(value) => Value::Number(Number::from(value.timestamp())),
                                None => Value::Null,
                            },
                        );
                        data.insert(
                            "uptime".to_string(),
                            match peer.connect_time {
                                Some(value) => Value::String(format_time_as_elapsed(value)),
                                None => Value::Null,
                            },
                        );
                        data.insert("state".to_string(), Value::String(peer.state.to_string()));
                        data.insert(
                            "prefixes_received".to_string(),
                            match peer.prefixes_received {
                                Some(value) => Value::Number(Number::from(value)),
                                None => Value::Null,
                            },
                        );
                        Value::Object(data)
                    })
                    .collect();
                *response.body_mut() =
                    Body::from(serde_json::to_string(&Value::Array(output)).unwrap());
            }
            Err(err) => {
                error!("{}", err);
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            }
        },
        (&Method::GET, "/show/routes/learned") => {
            match DB::new().and_then(|db| db.get_all_routes()) {
                Ok(routes) => {
                    let output: Vec<Value> = routes
                        .iter()
                        .map(|route| {
                            let mut data: Map<String, Value> = Map::new();
                            data.insert(
                                "received_from".to_string(),
                                Value::String(route.received_from.to_string()),
                            );
                            data.insert(
                                "received_at".to_string(),
                                Value::Number(Number::from(route.received_at.timestamp())),
                            );
                            data.insert(
                                "age".to_string(),
                                Value::String(format_time_as_elapsed(route.received_at)),
                            );
                            data.insert(
                                "prefix".to_string(),
                                Value::String(route.prefix.to_string()),
                            );
                            data.insert(
                                "next_hop".to_string(),
                                Value::String(route.next_hop.to_string()),
                            );
                            data.insert(
                                "origin".to_string(),
                                Value::String(route.next_hop.to_string()),
                            );
                            data.insert("as_path".to_string(), {
                                let as_path = route
                                    .as_path
                                    .segments
                                    .iter()
                                    .map(|segment| {
                                        let asns = match segment {
                                            Segment::AS_SEQUENCE(asns) => asns,
                                            Segment::AS_SET(asns) => asns,
                                        };
                                        asns.iter()
                                            .map(std::string::ToString::to_string)
                                            .collect::<Vec<String>>()
                                            .join(" ")
                                    })
                                    .collect::<Vec<String>>();
                                Value::String(as_path.join("; "))
                            });
                            data.insert(
                                "local_pref".to_string(),
                                match route.local_pref {
                                    Some(value) => Value::Number(Number::from(value)),
                                    None => Value::Null,
                                },
                            );
                            data.insert(
                                "multi_exit_disc".to_string(),
                                match route.multi_exit_disc {
                                    Some(value) => Value::Number(Number::from(value)),
                                    None => Value::Null,
                                },
                            );
                            data.insert(
                                "communities".to_string(),
                                Value::String(route.communities.to_string()),
                            );
                            Value::Object(data)
                        })
                        .collect();
                    *response.body_mut() =
                        Body::from(serde_json::to_string(&Value::Array(output)).unwrap());
                }
                Err(err) => {
                    error!("{}", err);
                    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };
    trace!("{} [{}]", req.uri(), response.status());
    Box::new(future::ok(response))
}
