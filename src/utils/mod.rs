mod display;
pub use display::*;
mod parse;
pub use parse::*;

/// Convert a u32 into an array of 4 bytes (BigEndian)
pub fn transform_u32_to_bytes(x: u32) -> [u8; 4] {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    [b1, b2, b3, b4]
}

/// Convert an array of 8 bytes into a u64 (BigEndian)
pub fn as_u64_be(array: [u8; 8]) -> u64 {
    (u64::from(array[0]) << 56)
    + (u64::from(array[1]) << 48)
    + (u64::from(array[2]) << 40)
    + (u64::from(array[3]) << 32)
    + (u64::from(array[4]) << 24)
    + (u64::from(array[5]) << 16)
    + (u64::from(array[6]) << 8)
    + u64::from(array[7])
}

/// Convert an array of 4 bytes into a u32 (BigEndian)
pub fn as_u32_be(array: [u8; 4]) -> u32 {
    (u32::from(array[0]) << 24)
    + (u32::from(array[1]) << 16)
    + (u32::from(array[2]) << 8)
    + u32::from(array[3])
}

/// Convert an array of 2 bytes into a u16 (BigEndian)
pub fn as_u16_be(array: [u8; 2]) -> u16 {
    (u16::from(array[0]) << 8) + u16::from(array[1])
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
        assert_eq!(as_u16_be([0, 180]), 180);
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_u8_transforms() {
        assert_eq!(as_u16_be([0x01, 0x2c]), 300); // hex
    }

    #[test]
    fn test_as_u32_be() {
        assert_eq!(as_u32_be([1, 1, 1, 1]), 16843009);
    }

    #[test]
    fn test_u32_to_dotted() {
        assert_eq!(u32_to_dotted(100, '.'), "100".to_string());
        assert_eq!(u32_to_dotted(4259840100, '.'), "65000.100".to_string());
    }
}
