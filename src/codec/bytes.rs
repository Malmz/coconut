use bytes::BytesMut;
use bytes::Bytes;
use bytes::BufMut;
use std::convert::Infallible;

pub struct BytesCodec {}

impl crate::Decoder for BytesCodec {
    type Item = BytesMut;
    fn decode(&mut self, buf: &mut BytesMut) -> Option<Self::Item> {
        if !buf.is_empty() {
            Some(buf.split_to(buf.len()))
        } else {
            None
        }
    }
}

impl crate::Encoder for BytesCodec {
    type Item = Bytes;
    type Error = Infallible;
    fn encode(&mut self, item: Self::Item, dest: &mut BytesMut) -> Result<(), Self::Error> {
        dest.reserve(item.len());
        dest.put(item);
        Ok(())
    }
}