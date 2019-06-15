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

}
