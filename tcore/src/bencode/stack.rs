use std::collections::BTreeMap;

use thiserror::Error;

use super::value::{ByteString, Value};

#[derive(Debug)]
pub struct Stack {
    stack: Vec<Container>,
}

#[derive(PartialEq, Debug, Error)]
pub enum StructureError {
    #[error("expected string as a key in dictionary, got SOMETHING FFUICKING ELSE")] //FIXME:
    PushToDictError,
    #[error("dictionary key has no value")]
    OrphanedKey,
}

impl Stack {
    pub fn new() -> Stack {
        Stack { stack: Vec::new() }
    }

    /// If stack is empty, returns the value back
    pub fn push_value(&mut self, v: Value) -> Result<Option<Value>, StructureError> {
        if let Some(top) = self.stack.last_mut() {
            if let Err(e) = top.push_value(v) {
                return Err(e);
            }
            Ok(None)
        } else {
            Ok(Some(v))
        }
    }

    pub fn push_list(&mut self) {
        self.stack.push(Container::new_list());
    }

    pub fn push_dict(&mut self) {
        self.stack.push(Container::new_dict());
    }

    /// Pops the top container from the stack, and if it was the last item on stack, returns it.
    pub fn pop_container(&mut self) -> Result<Option<Value>, StructureError> {
        match self.stack.pop() {
            Some(top) => self.push_value(top.to_value()?),
            None => Ok(None),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

#[derive(Debug)]
enum Container {
    List(Vec<Value>),
    Dict(DictBuilder),
}

impl Container {
    fn new_list() -> Container {
        Container::List(Vec::new())
    }

    fn new_dict() -> Container {
        Container::Dict(DictBuilder::new())
    }

    fn push_value(&mut self, v: Value) -> Result<(), StructureError> {
        match self {
            Self::List(l) => Ok(l.push(v)),
            Self::Dict(d) => d.insert(v),
        }
    }

    fn to_value(self) -> Result<Value, StructureError> {
        match self {
            Self::List(l) => Ok(l.into()),
            Self::Dict(d) => Ok(Value::Dictionary(d.finish()?.into())),
        }
    }
}

#[derive(Debug)]
struct DictBuilder {
    dict: Vec<(ByteString, Value)>,
    pending_key: Option<ByteString>,
}

impl DictBuilder {
    fn new() -> DictBuilder {
        DictBuilder {
            dict: Vec::new(),
            pending_key: None,
        }
    }

    fn insert(&mut self, v: Value) -> Result<(), StructureError> {
        match self.pending_key.take() {
            None => {
                if let Value::String(s) = v {
                    self.pending_key = Some(s);
                    Ok(())
                } else {
                    Err(StructureError::PushToDictError)
                }
            }
            Some(k) => {
                self.dict.push((k, v));
                Ok(())
            }
        }
    }

    fn finish(self) -> Result<Vec<(ByteString, Value)>, StructureError> {
        if self.pending_key != None {
            return Err(StructureError::OrphanedKey);
        }
        Ok(self.dict)
    }
}
