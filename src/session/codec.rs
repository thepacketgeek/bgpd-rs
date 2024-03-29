use std::io::{Error, Read};
use std::result::Result;

use bgp_rs::{Capabilities, Message, Reader};
use byteorder::{NetworkEndian, ReadBytesExt};
use bytes::{Buf, BufMut, BytesMut};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};
use twoway::find_bytes;

pub type MessageProtocol = Framed<TcpStream, MessageCodec>;

#[derive(Debug, Default)]
pub struct MessageCodec;

impl MessageCodec {
    pub fn new() -> Self {
        Self
    }

    fn get_reader<T>(&self, stream: T) -> Reader<T, Capabilities>
    where
        T: Read,
    {
        Reader::<T, Capabilities>::new(stream)
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    // Look for a BGP message (preamble + length), using bgp-rs to decode each message
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Error> {
        if let Ok(range) = find_msg_range(buf) {
            let mut reader = self.get_reader(&buf[range.start..range.stop]);
            let (_header, message) = reader.read()?;
            buf.advance(range.stop);
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = Error;

    fn encode(&mut self, message: Message, buf: &mut BytesMut) -> Result<(), Error> {
        message.encode(&mut buf.writer())
    }
}

#[derive(Debug)]
struct MsgRange {
    start: usize,
    stop: usize,
}

/// Given a stream of bytes, find the start and end of a BGP message
fn find_msg_range(data: &[u8]) -> Result<MsgRange, String> {
    if let Some(start) = find_bytes(data, &[255; 16]) {
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
}
