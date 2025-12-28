use std::{collections::HashMap, io::BufRead};

use bytes::{Buf, BufMut, BytesMut};

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

enum NestedType {
    List(ListBuilder),
    Dict(DictBuilder),
}

impl NestedType {
    fn to_value(self) -> Value {
        match self {
            Self::List(l) => Value::list(l.finish()),
            Self::Dict(d) => Value::dictionary(d.finish()),
        }
    }
}

struct ListBuilder {
    list: Vec<Value>,
}

impl ListBuilder {
    fn new() -> ListBuilder {
        ListBuilder { list: Vec::new() }
    }

    fn finish(self) -> Vec<Value> {
        self.list
    }
}

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

    fn set_key(&mut self, k: String) {
        self.pending_key = Some(k)
    }

    fn set_value(&mut self, v: Value) {
        if let Some(k) = self.pending_key.take() {
            self.dict.insert(k, v);
        }
    }

    fn finish(self) -> HashMap<String, Value> {
        self.dict
    }
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

    //FIXME: an error type
    pub fn decode(&mut self) -> Result<Value, &'static str> {
        loop {
            match self.state {
                DecoderState::NeedRefill => {
                    self.refill()?;
                    self.state = DecoderState::Running;
                }
                DecoderState::Running => {
                    if self.buf.len() == 0 {
                        self.state = DecoderState::NeedRefill;
                        continue;
                    }

                    let token = self.next()?;
                    match token {
                        Token::Int(i) => {
                            let i = Value::int(i);
                            if let Some(top) = self.stack.last_mut() {
                                match top {
                                    NestedType::List(l) => {
                                        l.list.push(i);
                                    }
                                    NestedType::Dict(d) => {
                                        if None == d.pending_key {
                                            return Err(
                                                "trying to insert integer as a key in dictionary",
                                            );
                                        }
                                        d.set_value(i);
                                    }
                                }
                            } else {
                                return Ok(i);
                            }
                        }
                        Token::String(str) => {
                            let str = Value::string(str);
                            if let Some(top) = self.stack.last_mut() {
                                match top {
                                    NestedType::List(l) => {
                                        l.list.push(str);
                                    }
                                    NestedType::Dict(d) => {
                                        if None == d.pending_key {
                                            return Err(
                                                "trying to insert integer as a key in dictionary",
                                            );
                                        }
                                        d.set_value(str);
                                    }
                                }
                            } else {
                                return Ok(str);
                            }
                        }
                        Token::BeginList => {
                            self.stack.push(NestedType::List(ListBuilder::new()));
                            if !self.buf.is_empty() {
                                self.buf.advance(1);
                            }
                        }
                        Token::BeginDict => {
                            self.stack.push(NestedType::Dict(DictBuilder::new()));
                            if !self.buf.is_empty() {
                                self.buf.advance(1);
                            }
                        }
                        Token::EndOfObj => {
                            if let Some(v) = self.stack.pop() {
                                if let Some(top) = self.stack.last_mut() {
                                    match top {
                                        NestedType::List(l) => {
                                            l.list.push(v.to_value());
                                        }
                                        NestedType::Dict(d) => {
                                            if None == d.pending_key {
                                                return Err(
                                                    "trying to insert integer as a key in dictionary",
                                                );
                                            }
                                            d.set_value(v.to_value());
                                        }
                                    }
                                } else {
                                    return Ok(v.to_value());
                                }
                            }
                            if !self.buf.is_empty() {
                                self.buf.advance(1);
                            }
                        }
                        Token::Invalid => return Err("syntax error"),
                    };
                }
            }
        }
    }

    //FIXME: an error type
    pub fn next<'a>(&mut self) -> Result<Token, &'static str> {
        let b = match self.buf.first() {
            Some(b) => b,
            None => {
                self.refill()?;
                match self.buf.first() {
                    Some(b) => b,
                    None => {
                        return Err("some error"); //FIXME:
                    }
                }
            }
        };

        match b {
            b'i' => parse_int(&self.buf),
            b'0'..=b'9' => parse_string(&self.buf),
            b'l' => Ok(Token::BeginList),
            b'd' => Ok(Token::BeginDict),
            b'e' => Ok(Token::EndOfObj),
            _ => Ok(Token::Invalid),
        }
    }

    //FIXME: an error type
    fn refill(&mut self) -> Result<(), &'static str> {
        let tmp = match self.src.fill_buf() {
            Ok(tmp) => tmp,
            Err(_) => return Err("can't refill"),
        };
        self.buf.put(tmp);
        Ok(())
    }
}

#[cfg(test)]
mod test_decoder {
    use super::*;

    #[test]
    fn valid_string() {
        let src = b"4:test";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode(), Ok(Value::string("test")));
    }

    #[test]
    fn valid_int() {
        let src = b"i42e";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(dec.decode(), Ok(Value::int(42)));
    }

    #[test]
    fn valid_flat_list() {
        let src = b"li42e4:teste";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode(),
            Ok(Value::list(vec![Value::int(42), Value::string("test")]))
        );
    }

    #[test]
    fn valid_flat_dict() {
        let src = b"d4:testi42ee";
        let mut dec = Decoder::new(&src[..]);
        assert_eq!(
            dec.decode(),
            Ok(Value::dictionary(HashMap::from([(
                String::from("test"),
                Value::int(42)
            )])))
        )
    }
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
//FIXME: an error type
fn parse_string(buf: &[u8]) -> Result<Token, &'static str> {
    let i = match buf.iter().position(|x| *x == b':') {
        Some(i) => i,
        None => return Err("invalid string"), // FIXME: not always true, might need more bytes
    };

    // 9_999_999 - 10 MB string, too large
    // NOTE: maybe i want to configure it
    if i > 7 {
        return Err("string is too big");
    }

    let len = match atoi::atoi(&buf[..i]) {
        Some(len) => len,
        None => return Err("syntax error in string length"),
    };

    if buf.len() - i + 1 < len {
        return Err("need more data"); // FIXME:
    }

    Ok(Token::String(
        String::from_utf8(buf[i + 1..].to_vec()).unwrap(),
    )) // FIXME:
}

//FIXME: an error type
pub fn parse_int(buf: &[u8]) -> Result<Token, &'static str> {
    let i = match buf.iter().position(|x| *x == b'e') {
        Some(i) => i,
        None => return Err("not enough bytes"), //FIXME:
    };

    // not including i and e
    if buf.len() - 2 > 12 {
        return Err("the number is too big");
    }

    match atoi::atoi(&buf[1..i]) {
        Some(n) => Ok(Token::Int(n)),
        None => Err("syntax error"),
    }
}

#[cfg(test)]
mod test_parsers {
    use super::*;

    #[test]
    fn valid_string() {
        let buf = b"4:test";
        assert_eq!(parse_string(buf), Ok(Token::String(String::from("test"))))
    }

    #[test]
    fn valid_int() {
        let buf = b"i42e";
        assert_eq!(parse_int(buf), Ok(Token::Int(42)))
    }
}
