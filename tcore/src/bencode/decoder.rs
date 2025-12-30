#![allow(unused)] // FIXME:

use std::{
    collections::HashMap,
    io::{self, BufRead},
};

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

use crate::bencode::stack;

// maybe i should just store references to bytes?
#[derive(PartialEq, Debug)]
pub enum Value {
    Int(isize),
    String(String),
    List(Vec<Box<Value>>),
    Dictionary(HashMap<String, Box<Value>>),
}

impl Value {
    pub fn int(i: isize) -> Self {
        Value::Int(i)
    }

    pub fn string<S: Into<String>>(s: S) -> Self {
        Value::String(s.into())
    }

    pub fn list(v: Vec<Value>) -> Self {
        Value::List(v.into_iter().map(Box::new).collect())
    }

    pub fn dictionary(v: HashMap<String, Value>) -> Self {
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
    stack: Vec<NestedType>,
}

enum DecoderState {
    Running,
    NeedRefill,
}

#[derive(Debug)]
enum NestedType {
    List(ListBuilder),
    Dict(DictBuilder),
}

impl NestedType {
    fn insert(&mut self, v: Value) {
        match self {
            Self::List(l) => l.insert(v),
            Self::Dict(d) => d.insert(v),
        }
    }

    fn to_value(self) -> Value {
        match self {
            Self::List(l) => Value::list(l.finish()),
            Self::Dict(d) => Value::dictionary(d.finish()),
        }
    }
}

#[derive(Debug)]
struct ListBuilder {
    list: Vec<Value>,
}

impl ListBuilder {
    fn new() -> ListBuilder {
        ListBuilder { list: Vec::new() }
    }

    fn insert(&mut self, v: Value) {
        self.list.push(v);
    }

    fn finish(self) -> Vec<Value> {
        self.list
    }
}

#[derive(Debug)]
struct DictBuilder {
    dict: HashMap<String, Value>,
    pending_key: Option<String>,
}

impl DictBuilder {
    fn new() -> DictBuilder {
        DictBuilder {
            dict: HashMap::new(),
            pending_key: None,
        }
    }

    fn insert(&mut self, v: Value) {
        match self.pending_key.take() {
            None => {
                if let Value::String(s) = v {
                    self.pending_key = Some(s);
                } else {
                    panic!("inserting non-string value as a key in dictionary");
                    // FIXME: maybe should not panic
                }
            }
            Some(k) => {
                self.dict.insert(k, v);
            }
        }
    }

    fn finish(self) -> HashMap<String, Value> {
        self.dict
    }
}

#[derive(Error, Debug)]
pub enum DecoderError {
    #[error("wrong syntax")]
    WrongSyntax,
    #[error("the value is too large")]
    ValueTooLarge,
    #[error("unable to read file: {0}")]
    Io(#[from] io::Error),
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
            stack: Vec::new(),
        }
    }

    pub fn with_capacity(src: T, cap: usize) -> Decoder<T> {
        Decoder {
            src: src,
            buf: BytesMut::with_capacity(cap),
            state: DecoderState::NeedRefill,
            stack: Vec::new(),
        }
    }

    //FIXME: CLEAN THAT BARELY READABLE MESS
    pub fn decode(&mut self) -> Result<Value, DecoderError> {
        loop {
            match self.state {
                DecoderState::NeedRefill => {
                    self.refill()?;
                    self.state = DecoderState::Running;
                }

                DecoderState::Running => {
                    let maybe_token = self.next()?;

                    match maybe_token {
                        Some(token) => {
                            match token {
                                Token::Int(i) => {
                                    let i = Value::int(i);
                                    match self.stack.last_mut() {
                                        Some(top) => top.insert(i),
                                        None => return Ok(i),
                                    }
                                }
                                Token::String(str) => {
                                    let str = Value::string(str);
                                    match self.stack.last_mut() {
                                        Some(top) => top.insert(str),
                                        None => return Ok(str),
                                    }
                                }
                                Token::BeginList => {
                                    self.stack.push(NestedType::List(ListBuilder::new()));
                                }
                                Token::BeginDict => {
                                    self.stack.push(NestedType::Dict(DictBuilder::new()));
                                }
                                Token::EndOfObj => {
                                    if let Some(v) = self.stack.pop() {
                                        let v = v.to_value();
                                        match self.stack.last_mut() {
                                            Some(top) => top.insert(v),
                                            None => return Ok(v),
                                        }
                                    }
                                }
                                Token::Invalid => return Err(DecoderError::WrongSyntax),
                            };
                        }
                        None => self.state = DecoderState::NeedRefill,
                    }
                }
            }
        }
    }

    /// returns None if it needs more bytes
    pub fn next<'a>(&mut self) -> Result<Option<Token>, DecoderError> {
        let b = match self.buf.first() {
            Some(b) => b,
            None => return Ok(None),
        };

        match b {
            b'i' => {
                let (token, len) = match parse_int(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(token)
            }
            b'0'..=b'9' => {
                let (token, len) = match parse_string(&self.buf) {
                    Ok(ok) => ok,
                    Err(e) => return Err(e),
                };
                self.advance_buff(len);
                Ok(token)
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

    fn refill(&mut self) -> Result<(), DecoderError> {
        let tmp = match self.src.fill_buf() {
            Ok(tmp) => tmp,
            Err(e) => return Err(DecoderError::Io(e)),
        };
        let len = tmp.len();
        if len > 0 {
            self.buf.extend_from_slice(tmp);
            self.src.consume(len);
        }
        Ok(())
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
        assert_eq!(dec.decode().unwrap(), Value::string("test"));
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
            Value::list(vec![Value::int(42), Value::string("test")])
        );
    }

    //TODO: test nested lists

    #[test]
    fn valid_flat_dict() {
        let src = b"d4:testi42ee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode().unwrap(),
            Value::dictionary(HashMap::from([(String::from("test"), Value::int(42))]))
        )
    }

    //TODO: test nested dicts

    //TODO: test nested lists and dicts combined
}

#[derive(PartialEq, Debug)]
pub enum Token {
    Int(isize),
    String(String),
    BeginList,
    BeginDict, // Cumberbatch
    EndOfObj,
    Invalid,
}

/// expects string token
fn parse_string(buf: &[u8]) -> Result<(Option<Token>, usize), DecoderError> {
    let i = match buf.iter().position(|x| *x == b':') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    // 9_999_999 - 10 MB string, too large
    // NOTE: maybe i want to configure it
    if i > 7 {
        return Err(DecoderError::ValueTooLarge);
    }

    let len = match atoi::atoi(&buf[..i]) {
        Some(len) => len,
        None => return Err(DecoderError::WrongSyntax),
    };

    if buf.len() - i + 1 < len {
        return Ok((None, 0));
    }

    Ok((
        Some(Token::String(
            String::from_utf8(buf[i + 1..i + 1 + len].to_vec()).unwrap(),
        )),
        i + len + 1,
    )) // FIXME:
}

pub fn parse_int(buf: &[u8]) -> Result<(Option<Token>, usize), DecoderError> {
    let i = match buf.iter().position(|x| *x == b'e') {
        Some(i) => i,
        None => return Ok((None, 0)),
    };

    // not including i and e
    if buf.len() - 2 > 12 {
        return Err(DecoderError::ValueTooLarge);
    }

    match atoi::atoi(&buf[1..i]) {
        Some(n) => Ok((Some(Token::Int(n)), i + 1)),
        None => Err(DecoderError::WrongSyntax),
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
            (Some(Token::String(String::from("test"))), 6 as usize)
        )
    }

    #[test]
    fn valid_int() {
        let buf = b"i42e";
        assert_eq!(parse_int(buf).unwrap(), (Some(Token::Int(42)), 4))
    }

    //TODO: test for failing cases
}
