use std::net::IpAddr;

use chrono::{DateTime, Duration, TimeZone, Utc};

use super::*;

pub fn get_message_type(message: &Message) -> &'static str {
    match message {
        Message::KeepAlive => "KEEPALIVE",
        Message::Open(_) => "OPEN",
        Message::Notification(_) => "NOTIFICATION",
        Message::RouteRefresh(_) => "ROUTEREFRESH",
        Message::Update(_) => "UPDATE",
    }
}

/// Convert an ASN (4 byte) as dotted if it exceeds the 2-byte limit
/// E.g. 42598400100 -> "65000.100"
pub fn u32_to_dotted(asn: u32, sep: char) -> String {
    if asn < std::u16::MAX as u32 {
        format!("{}", asn)
    } else {
        let bytes = transform_u32_to_bytes(asn);
        format!(
            "{}{}{}",
            as_u16_be([bytes[0], bytes[1]]),
            sep,
            as_u16_be([bytes[2], bytes[3]])
        )
    }
}

/// Convert first 16 bytes (1 IPv6 address) to IpAddr
/// TODO: Handle multiple next hops
///       Can they be variable length?
pub fn bytes_to_ipv6(bytes: &Vec<u8>) -> IpAddr {
    let mut buffer: [u8; 16] = [0; 16];
    buffer[..16].clone_from_slice(&bytes[..16]);
    IpAddr::from(buffer)
}

fn fit_with_remainder(dividend: u64, divisor: u64) -> (u64, u64) {
    let fit = dividend / divisor;
    let remainder = dividend % divisor;
    (fit, remainder)
}

pub fn get_elapsed_time<Tz>(time: DateTime<Tz>) -> Duration
where
    Tz: TimeZone,
{
    Utc::now().signed_duration_since(time)
}

/// Given a duration, format like "00:00:00"
pub fn format_elapsed_time(elapsed: Duration) -> String {
    let elapsed = elapsed.num_seconds().abs() as u64;
    let (hours, remainder) = fit_with_remainder(elapsed, 3600);
    let (minutes, seconds) = fit_with_remainder(remainder, 60);
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// Given a timestamp, get the elapsed time and return formatted string
pub fn format_time_as_elapsed<Tz>(time: DateTime<Tz>) -> String
where
    Tz: TimeZone,
{
    format_elapsed_time(get_elapsed_time(time))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_to_dotted() {
        assert_eq!(u32_to_dotted(100, '.'), "100".to_string());
        assert_eq!(u32_to_dotted(4259840100, '.'), "65000.100".to_string());
    }
}
