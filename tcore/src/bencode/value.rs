use std::{borrow::Cow, collections::BTreeMap, fmt::Display};

/// ByteString - bencoded string as byte sequence
pub type ByteString = Vec<u8>;

#[derive(PartialEq, Debug)]
pub enum Value {
    Int(i64),
    String(ByteString),
    List(Vec<Value>),
    Dictionary(Vec<(ByteString, Value)>),
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Int(_) => write!(f, "int"),
            Self::String(_) => write!(f, "string"),
            Self::List(_) => write!(f, "list"),
            Self::Dictionary(_) => write!(f, "dictionary"),
        }
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Int(value as i64)
    }
}

impl From<&[u8]> for Value {
    fn from(value: &[u8]) -> Self {
        Self::String(value.to_vec())
    }
}

impl From<& str> for Value {
    fn from(value: & str) -> Self {
        Self::String(value.as_bytes().to_vec())
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::List(value)
    }
}

impl From<Vec<(ByteString, Value)>> for Value {
    fn from(value: Vec<(ByteString, Value)>) -> Self {
        Self::Dictionary(value)
    }
}

impl PartialEq<i64> for Value {
    fn eq(&self, other: &i64) -> bool {
        matches!(self, Self::Int(v) if v == other)
    }
}

impl PartialEq<ByteString> for Value {
    fn eq(&self, other: &ByteString) -> bool {
        matches!(self, Self::String(v) if v == other)
    }
}

impl PartialEq<&[u8]> for Value {
    fn eq(&self, other: &&[u8]) -> bool {
        matches!(self, Self::String(v) if v == other)
    }
}

impl PartialEq<Vec<Value>> for Value {
    fn eq(&self, other: &Vec<Value>) -> bool {
        matches!(self, Self::List(l) if l == other)
    }
}

