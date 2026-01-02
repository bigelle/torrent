use std::{collections::BTreeMap, fmt::Display};

/// ByteString - bencoded string as byte sequence
pub type ByteString = Vec<u8>;

#[derive(PartialEq, Debug)]
pub enum Value {
    Int(i64),
    String(ByteString),
    List(Vec<Box<Value>>),
    Dictionary(BTreeMap<ByteString, Box<Value>>),
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

impl Value {
    pub fn int<A: Into<i64>>(i: A) -> Self {
        Value::Int(i.into())
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

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.into())
    }
}
