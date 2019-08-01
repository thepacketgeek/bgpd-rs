use std::io::{Error, Read};
use std::result::Result;

use bgp_rs::{Capabilities, Message, Open, OpenParameter, Reader};
use byteorder::{NetworkEndian, ReadBytesExt};
use bytes::{BufMut, BytesMut};
use log::warn;
use tokio::net::TcpStream;
use tokio_codec::{Decoder, Encoder, Framed};
use twoway::find_bytes;

use crate::utils::*;

pub type MessageProtocol = Framed<TcpStream, MessageCodec>;

#[derive(Default)]
pub struct MessageCodec {
    pub capabilities: Capabilities,
}

impl MessageCodec {
    pub fn new() -> MessageCodec {
        MessageCodec {
            capabilities: Capabilities::default(),
        }
    }

    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
    }

    fn get_reader<T>(&self, stream: T) -> Reader<T>
    where
        T: Read,
    {
        Reader::<T> {
            stream,
            capabilities: self.capabilities,
        }
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    // Look for a BGP message (preamble + length), using bgp-rs to decode each message
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if let Ok(range) = find_msg_range(&buf) {
            let mut reader = self.get_reader(&buf[range.start..range.stop]);
            if let Ok((_header, message)) = reader.read() {
                buf.advance(range.stop);
                return Ok(Some(message));
            }
        }
        Ok(None)
    }
}

impl Encoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn encode(&mut self, message: Message, buf: &mut BytesMut) -> Result<(), Error> {
        if let Some(data) = match message {
            Message::Open(open) => Some(encode_open(open)),
            Message::KeepAlive => Some(encode_keepalive()),
            _ => {
                warn!("Message type '{:?}' not supported", message);
                None
            }
        } {
            buf.reserve(data.len() + 1);
            buf.put(data);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct MsgRange {
    start: usize,
    stop: usize,
}

/// Given a stream of bytes, find the start and end of a BGP message
fn find_msg_range(data: &[u8]) -> Result<MsgRange, String> {
    if let Some(start) = find_bytes(&data, &[255; 16]) {
        let buf = &mut (*data).split_at(start).1;
        let mut _preamble: [u8; 16] = [0; 16];
        let _ = buf.read_exact(&mut _preamble);
        let length = buf.read_u16::<NetworkEndian>().unwrap();
        Ok(MsgRange {
            start,
            stop: start + (length as usize),
        })
    } else {
        Err("Couldn't determine BGP message start/stop (No preamble found)".to_string())
    }
}

fn prepend_preamble_and_length(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut preamble: Vec<u8> = vec![0xff; 16];
    let length: u16 = (bytes.len() as u16) + (preamble.len() as u16) + 2;
    preamble.extend_from_slice(&transform_u16_to_bytes(length));
    preamble.append(&mut bytes);
    preamble
}

fn encode_open_parameter(param: &OpenParameter) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![];
    bytes.extend_from_slice(&transform_u8_to_bytes(param.param_type));
    bytes.extend_from_slice(&transform_u8_to_bytes(param.param_length));
    bytes.extend_from_slice(&param.value);
    bytes
}

fn encode_open(open: Open) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![];
    bytes.extend_from_slice(&[1]); // type, Open
    bytes.extend_from_slice(&[open.version]);
    bytes.extend_from_slice(&transform_u16_to_bytes(open.peer_asn));
    bytes.extend_from_slice(&transform_u16_to_bytes(open.hold_timer));
    bytes.extend_from_slice(&transform_u32_to_bytes(open.identifier));

    let params: Vec<u8> = open
        .parameters
        .iter()
        .map(|p| encode_open_parameter(&p))
        .flatten()
        .collect();
    bytes.extend_from_slice(&[params.len() as u8]); // Optional Parameters Length
    bytes.extend_from_slice(&params);
    prepend_preamble_and_length(bytes)
}

fn encode_keepalive() -> Vec<u8> {
    prepend_preamble_and_length(vec![4]) // type, Keepalive
}

pub fn capabilities_from_params(parameters: &[OpenParameter]) -> (Capabilities, Option<u32>) {
    let mut asn_4_byte: Option<u32> = None;
    let mut capabilities = Capabilities {
        FOUR_OCTET_ASN_SUPPORT: false,
        ..Capabilities::default()
    };
    for param in parameters.iter().filter(|p| p.param_type == 2) {
        match param.value[0] {
            65 => {
                asn_4_byte = (&param.value[2..6]).read_u32::<NetworkEndian>().ok();
                capabilities.FOUR_OCTET_ASN_SUPPORT = true;
            }
            69 => {
                capabilities.EXTENDED_PATH_NLRI_SUPPORT = true;
            }
            _ => {}
        }
    }
    (capabilities, asn_4_byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_msg_range() {
        let data: [u8; 64] = [
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 45,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let range = find_msg_range(&data).unwrap();
        assert_eq!(range.start, 0);
        assert_eq!(range.stop, 45);
    }

    #[test]
    fn test_find_msg_range_err() {
        let data: [u8; 32] = [
            0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ];
        let range = find_msg_range(&data);
        assert!(range.is_err());
    }

    #[test]
    fn test_encode_open_parameter() {
        let param = OpenParameter {
            // 4-byte ASN
            param_type: 2,
            param_length: 6,
            value: vec![0x41, 0x04, 0x00, 0x02, 0xfd, 0xe8],
        };
        let data = encode_open_parameter(&param);
        assert_eq!(data, vec![2, 6, 65, 4, 0, 2, 253, 232]);
    }

    #[test]
    fn test_encode_keepalive() {
        let data = encode_keepalive();
        assert_eq!(
            data,
            vec![
                // preamble
                255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0,
                19, // length
                4,  // type
            ]
        );
    }
}
