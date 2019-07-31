use chrono::{DateTime, Duration, Utc};
use std::net::IpAddr;

pub const EMPTY_VALUE: &str = "";

pub fn transform_u8_to_bytes(x: u8) -> [u8; 1] {
    let b1: u8 = x as u8;
    [b1]
}

pub fn transform_u16_to_bytes(x: u16) -> [u8; 2] {
    let b1: u8 = ((x >> 8) & 0xff) as u8;
    let b2: u8 = (x & 0xff) as u8;
    [b1, b2]
}

pub fn transform_u32_to_bytes(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    [b1, b2, b3, b4]
}

pub fn as_u32_be(array: [u8; 4]) -> u32 {
    (u32::from(array[0]) << 24)
        + (u32::from(array[1]) << 16)
        + (u32::from(array[2]) << 8)
        + u32::from(array[3])
}

pub fn as_u16_be(array: [u8; 2]) -> u16 {
    (u16::from(array[0]) << 8) + u16::from(array[1])
}

pub fn asn_to_dotted(asn: u32) -> String {
    if asn < 65535 {
        format!("{}", asn)
    } else {
        let bytes = transform_u32_to_bytes(asn);
        format!(
            "{}.{}",
            as_u16_be([bytes[0], bytes[1]]),
            as_u16_be([bytes[2], bytes[3]])
        )
    }
}

pub fn ext_community_to_display(value: u64) -> String {
    let c_type: u16 = ((value >> 48) & 0xff) as u16;
    match c_type {
        // 2-octet AS Specific Extended Community (RFC 4360)
        0x0 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let community: u32 = (value & 0xffff_ffff) as u32;
            format!("{}:{}", asn, asn_to_dotted(community))
        }
        // IPv4 Address Specific Extended Community (RFC 4360)
        0x1 => {
            let addr: u32 = ((value >> 24) & 0xffff_ffff) as u32;
            let asn: u16 = (value & 0xffff) as u16;
            format!("{}:{}", IpAddr::from(transform_u32_to_bytes(addr)), asn)
        }
        // 4-octet AS Specific BGP Extended Community (RFC 5668)
        0x2 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let addr: u32 = (value & 0xffff_ffff) as u32;
            format!(
                "target:{}:{}",
                asn,
                IpAddr::from(transform_u32_to_bytes(addr))
            )
        }
        // Opaque Extended Community (RFC 4360)
        0x3 => format!("opaque:{}", value),
        // Flow-Spec Redirect community
        0x8 => {
            let asn: u16 = ((value >> 32) & 0xffff) as u16;
            let number: u32 = (value & 0xffff_ffff) as u32;
            format!("redirect:{}:{}", asn, asn_to_dotted(number))
        }
        _ => format!("unknown:{}:{}", c_type, value),
    }
}

fn fit_with_remainder(dividend: u64, divisor: u64) -> (u64, u64) {
    let fit = dividend / divisor;
    let remainder = dividend % divisor;
    (fit, remainder)
}

pub fn get_elapsed_time(time: DateTime<Utc>) -> Duration {
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
pub fn format_time_as_elapsed(time: DateTime<Utc>) -> String {
    format_elapsed_time(get_elapsed_time(time))
}

pub fn maybe_string<T>(item: Option<&T>) -> String
where
    T: ToString,
{
    item.map(std::string::ToString::to_string)
        .unwrap_or_else(|| String::from(EMPTY_VALUE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u32_transforms() {
        assert_eq!(transform_u32_to_bytes(65000), [0, 0, 253, 232]); // u8
        assert_eq!(as_u32_be([0, 0, 253, 232]), 65000);

        assert_eq!(transform_u32_to_bytes(65300), [0x0, 0x0, 0xff, 0x14]); // hex
        assert_eq!(as_u32_be([0x0, 0x0, 0xff, 0x14]), 65300);
    }

    #[test]
    fn test_u16_transforms() {
        assert_eq!(transform_u16_to_bytes(180), [0, 180]); // u8
        assert_eq!(as_u16_be([0, 180]), 180);

        assert_eq!(transform_u16_to_bytes(300), [0x01, 0x2c]); // u8
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_u8_transforms() {
        assert_eq!(transform_u8_to_bytes(180), [180]);
        assert_eq!(transform_u8_to_bytes(252), [0xfc]); // u8
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_as_u32_be() {
        assert_eq!(as_u32_be([1, 1, 1, 1]), 16843009);
    }

    #[test]
    fn test_asn_to_dotted() {
        assert_eq!(asn_to_dotted(100), "100".to_string());
        assert_eq!(asn_to_dotted(4259840100), "65000.100".to_string());
    }

    #[test]
    fn test_format_elapsed_time() {
        assert_eq!(
            format_elapsed_time(Duration::seconds(30)),
            "00:00:30".to_string()
        );
        assert_eq!(
            format_elapsed_time(Duration::seconds(301)),
            "00:05:01".to_string()
        );
        assert_eq!(
            format_elapsed_time(Duration::seconds(32768)),
            "09:06:08".to_string()
        );
    }

    #[test]
    fn test_maybe_string() {
        let value: Option<u64> = Some(5);
        assert_eq!(maybe_string(value.as_ref()), String::from("5"));
        let value: Option<&str> = Some("test");
        assert_eq!(maybe_string(value.as_ref()), String::from("test"));
        let value: Option<&str> = None;
        assert_eq!(maybe_string(value.as_ref()), String::from(EMPTY_VALUE));
    }

    #[test]
    fn test_ext_community_to_display() {
        let two_byte_asn: u64 =
            u64::from_be_bytes([0x00, 0x00, 0xfd, 0xe8, 0x00, 0x64, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(two_byte_asn),
            String::from("65000:100.100")
        );

        let ipv4_comm: u64 = u64::from_be_bytes([0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(ipv4_comm),
            String::from("1.1.1.1:100")
        );

        let target: u64 = u64::from_be_bytes([0x00, 0x02, 0xfd, 0xe8, 0x01, 0x01, 0x01, 0x01]);
        assert_eq!(
            ext_community_to_display(target),
            String::from("target:65000:1.1.1.1")
        );

        let redirect: u64 = u64::from_be_bytes([0x80, 0x08, 0xfd, 0xe8, 0x00, 0x00, 0x00, 0x64]);
        assert_eq!(
            ext_community_to_display(redirect),
            String::from("redirect:65000:100")
        );
    }

}
