use std::fmt::Display;
use std::error::Error;

#[derive(Debug)]
pub enum FramesError<E: Error> {
    Io(futures::io::Error),
    Encode(E),
}

impl<E: Error> Display for FramesError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            FramesError::Io(err) => err.fmt(f),
            FramesError::Encode(err) => write!(f, "Encode error: {:?}", err),
        }
    }
}

impl<E: Error + 'static> Error for FramesError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FramesError::Io(err) => Some(err),
            FramesError::Encode(err) => Some(err),
        }
    }
}

impl<E: Error> From<futures::io::Error> for FramesError<E> {
    fn from(val: futures::io::Error) -> Self {
        FramesError::Io(val)
    }
}