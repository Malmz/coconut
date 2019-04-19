#![feature(futures_api)]

mod error;
pub mod codec;

use error::FramesError;

use std::task::{ Context, Poll };
use std::pin::Pin;

use bytes::BytesMut;
use bytes::BufMut;

use futures::ready;

use futures::{
    Stream,
    Sink,
    io::{
        AsyncRead,
        AsyncWrite,
    }
};

pub trait Decoder {
    type Item;
    fn decode(&mut self, buf: &mut BytesMut) -> Option<Self::Item>;
    fn decode_eof(&mut self, buf: &mut BytesMut) -> Option<Self::Item> {
        self.decode(buf)
    }
}

pub trait Encoder {
    type Item;
    type Error;
    fn encode(&mut self, item: Self::Item, dest: &mut BytesMut) -> Result<(), Self::Error>;
}

struct FramesInner<C> {
    eof: bool,
    is_readable: bool,
    codec: C,
    rb: BytesMut,
    wb: BytesMut,
}

impl<C> FramesInner<C> {
    fn new(codec: C) -> Self {
        Self {
            eof: false,
            is_readable: false,
            codec,
            rb: BytesMut::with_capacity(8 * 1024),
            wb: BytesMut::new(),
        }
    }
}

impl<C> FramesInner<C> 
    where C: Encoder 
{
    fn encode(&mut self, item: C::Item) -> Result<(), C::Error> {
        self.codec.encode(item, &mut self.wb)
    }
}

impl<C> FramesInner<C> where C: Decoder {
    fn decode(&mut self) -> Option<C::Item> {
        self.codec.decode(&mut self.rb)
    }

    fn decode_eof(&mut self) -> Option<C::Item> {
        self.codec.decode_eof(&mut self.rb)
    }
}

pub struct Frames<S, C> {
    source: S,
    inner: FramesInner<C>,
}

impl<S, C> Frames<S, C> {
    fn both<'a>(self: Pin<&'a mut Self>) -> (Pin<&'a mut S>, &'a mut FramesInner<C>) {
        unsafe {
            let Self {
                source: ref mut s,
                inner: ref mut i,
            } = self.get_unchecked_mut();
            (Pin::new_unchecked(s), i)
        }
    }

    pub fn new(source: S, codec: C) -> Self {
        Self {
            source,
            inner: FramesInner::new(codec),
        }
    }
}

impl<S, C> Stream for Frames<S, C> 
    where
        S: AsyncRead,
        C: Decoder,
{
    type Item = Result<C::Item, futures::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let (s, i): (Pin<&mut S>, &mut FramesInner<C>) = self.as_mut().both();
            if i.is_readable {
                if i.eof {
                    if let Some(frame) = i.decode_eof() {
                        return Poll::Ready(Some(Ok(frame)));
                    } else {
                        return Poll::Ready(None);
                    }
                }

                if let Some(frame) = i.decode() {
                    return Poll::Ready(Some(Ok(frame)))
                }
                i.is_readable = false;
            }
            assert!(!i.eof);

            i.rb.reserve(1);
            let n = unsafe {
                s.initializer().initialize(i.rb.bytes_mut());
                let n = ready!(s.poll_read(cx, i.rb.bytes_mut()))?;
                i.rb.advance_mut(n);
                n
            };
            if 0 == n {
                i.eof = true;
            }
            i.is_readable = true;
        }
    }
}



impl<S, C, T, E> Sink<T> for Frames<S, C> 
    where
        S: AsyncWrite,
        C: Encoder<Item=T, Error=E>,
        E: std::error::Error
{
    type SinkError = FramesError<C::Error>;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::SinkError>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::SinkError> {
        unsafe {
            self.get_unchecked_mut().inner.encode(item).map_err(FramesError::Encode)
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::SinkError>> {
        let (mut s, i): (Pin<&mut S>, &mut FramesInner<C>) = self.as_mut().both();
        while !i.wb.is_empty() {
            let n = ready!(s.as_mut().poll_write(cx, &i.wb))?;
            if n == 0 {
                return Poll::Ready(Err(futures::io::Error::new(
                        futures::io::ErrorKind::WriteZero,
                        "failed to write frame to transport",
                    ).into()));
            }
            i.wb.advance(n);
        }
        ready!(s.poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::SinkError>> {
        ready!(self.as_mut().poll_flush(cx))?;
        unsafe {
            self.map_unchecked_mut(|x| &mut x.source).poll_close(cx).map_err(FramesError::Io)
        }
    }
}