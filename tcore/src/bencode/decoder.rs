#![allow(unused)] // FIXME:

use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    io::{self, BufRead, ErrorKind},
};

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

use crate::bencode::stack::{self, Stack};

/// ByteString - bencoded string as byte sequence
pub type ByteString = Vec<u8>;

#[derive(PartialEq, Debug)]
pub enum Value {
    Int(isize),
    String(ByteString),
    List(Vec<Box<Value>>),
    Dictionary(BTreeMap<ByteString, Box<Value>>),
}

impl Value {
    pub fn int(i: isize) -> Self {
        Value::Int(i)
    }

    pub fn string<A: Into<ByteString>>(v: A) -> Self {
        Value::String(v.into())
    }

    pub fn string_ref<A: AsRef<[u8]>>(v: A) -> Self {
        Value::String(v.as_ref().to_vec())
    }

    pub fn list(v: Vec<Value>) -> Self {
        Value::List(v.into_iter().map(Box::new).collect())
    }

    pub fn dictionary(v: BTreeMap<ByteString, Value>) -> Self {
        Value::Dictionary(v.into_iter().map(|(k, v)| (k, Box::new(v))).collect())
    }
}

pub struct Decoder<T>
where
    T: BufRead,
{
    src: T,
    buf: BytesMut,
    state: DecoderState,
    stack: Stack,
}

enum DecoderState {
    Running,
    NeedRefill,
}

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("invalid bencode syntax")]
    InvalidSyntax,
    #[error("bencoded value is too large")]
    ValueTooLarge,
    #[error("unable to read file: {0}")]
    Io(#[from] io::Error),
    #[error("unexpected EOF")]
    UnexpectedEof,
}

impl<T> Decoder<T>
where
    T: BufRead,
{
    pub fn new(src: T) -> Decoder<T> {
        Decoder {
            src: src,
            buf: BytesMut::with_capacity(4096),
            state: DecoderState::NeedRefill,
            stack: Stack::new(),
        }
    }

    pub fn with_capacity(src: T, cap: usize) -> Decoder<T> {
        Decoder {
            src: src,
            buf: BytesMut::with_capacity(cap),
            state: DecoderState::NeedRefill,
            stack: Stack::new(),
        }
    }

    pub fn decode(&mut self) -> Result<Value, DecodeError> {
        loop {
            match self.state {
                DecoderState::NeedRefill => {
                    let len = self.refill()?;
                    if len == 0 {
                        return Err(DecodeError::UnexpectedEof);
                    }
                    self.state = DecoderState::Running;
                }

                DecoderState::Running => {
                    let maybe_token = self.next()?;

                    match maybe_token {
                        Some(token) => {
                            match token {
                                Token::Int(v) => {
                                    if let Some(v) = self.stack.push_value(Value::Int(v)) {
                                        return Ok(v);
                                    }
                                }
                                Token::String(v) => {
                                    if let Some(v) = self.stack.push_value(Value::String(v)) {
                                        return Ok(v);
                                    }
                                }
                                Token::BeginList => {
                                    self.stack.push_list();
                                }
                                Token::BeginDict => {
                                    self.stack.push_dict();
                                }
                                Token::EndOfObj => {
                                    if let Some(v) = self.stack.pop_container() {
                                        return Ok(v);
                                    }
                                }
                                Token::Invalid => return Err(DecodeError::InvalidSyntax),
                            };
                        }
                        None => self.state = DecoderState::NeedRefill,
                    }
                }
            }
        }
    }

    /// returns None if it needs more bytes
    pub fn next<'a>(&mut self) -> Result<Option<Token>, DecodeError> {
        let b = match self.buf.first() {
            Some(b) => b,
            None => return Ok(None),
        };

        match b {
            b'i' => {
                let (maybe_token, len) = match parse_int(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(maybe_token)
            }
            b'0'..=b'9' => {
                let (maybe_token, len) = match parse_string(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(maybe_token)
            }
            b'l' => {
                self.advance_buff(1);
                Ok(Some(Token::BeginList))
            }
            b'd' => {
                self.advance_buff(1);
                Ok(Some(Token::BeginDict))
            }
            b'e' => {
                self.advance_buff(1);
                Ok(Some(Token::EndOfObj))
            }
            _ => Ok(Some(Token::Invalid)),
        }
    }

    fn refill(&mut self) -> Result<usize, DecodeError> {
        let tmp = match self.src.fill_buf() {
            Ok(tmp) => tmp,
            Err(e) => return Err(DecodeError::Io(e)),
        };
        let len = tmp.len();
        if len > 0 {
            self.buf.extend_from_slice(tmp);
            self.src.consume(len);
        }
        Ok(len)
    }

    fn advance_buff(&mut self, n: usize) {
        if self.buf.len() >= n {
            self.buf.advance(n);
        }
    }
}

#[cfg(test)]
mod test_decoder {
    use super::*;

    #[test]
    fn valid_string() {
        let src = b"4:test";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::string(Vec::from("test")));
    }

    #[test]
    fn valid_int() {
        let src = b"i42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode().unwrap(), Value::int(42));
    }

    #[test]
    fn valid_flat_list() {
        let src = b"li42e4:teste";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::list(vec![Value::int(42), Value::string(Vec::from("test"))])
        );
    }

    //TODO: test nested lists

    #[test]
    fn valid_flat_dict() {
        let src = b"d4:testi42ee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::dictionary(BTreeMap::from([(Vec::from("test"), Value::int(42))]))
        )
    }

    //TODO: test nested dicts

    //TODO: test nested lists and dicts combined
}

#[derive(PartialEq, Debug)]
pub enum Token {
    Int(isize),
    String(ByteString),
    BeginList,
    BeginDict, // Cumberbatch
    EndOfObj,
    Invalid,
}

/// expects string token
fn parse_string(buf: &[u8]) -> Result<(Option<Token>, usize), DecodeError> {
    let i = match buf.iter().position(|x| *x == b':') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    // 9_999_999 - 10 MB string, too large
    // NOTE: maybe i want to configure it
    if i > 7 {
        return Err(DecodeError::ValueTooLarge);
    }

    let len = match atoi::atoi(&buf[..i]) {
        Some(len) => len,
        None => return Err(DecodeError::InvalidSyntax),
    };

    if buf.len() - i + 1 < len {
        return Ok((None, 0));
    }

    Ok((
        Some(Token::String(buf[i + 1..i + 1 + len].to_vec())),
        i + len + 1,
    ))
}

pub fn parse_int(buf: &[u8]) -> Result<(Option<Token>, usize), DecodeError> {
    let i = match buf.iter().position(|x| *x == b'e') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    // not including i and e
    if buf.len() - 2 > 12 {
        return Err(DecodeError::ValueTooLarge);
    }

    match atoi::atoi(&buf[1..i]) {
        Some(n) => Ok((Some(Token::Int(n)), i + 1)),
        None => Err(DecodeError::InvalidSyntax),
    }
}

#[cfg(test)]
mod test_parsers {
    use super::*;

    #[test]
    fn valid_string() {
        let buf = b"4:test";
        assert_eq!(
            parse_string(buf).unwrap(),
            (Some(Token::String(Vec::from("test"))), 6 as usize)
        )
    }

    #[test]
    fn valid_int() {
        let buf = b"i42e";
        assert_eq!(parse_int(buf).unwrap(), (Some(Token::Int(42)), 4))
    }

    //TODO: test for failing cases
}
