use std::collections::HashMap;

use crate::bencode::decoder::Value;

pub struct Stack {
    stack: Vec<Container>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack { stack: Vec::new() }
    }

    /// If stack is empty, returns the value back
    pub fn push_value(&mut self, v: Value) -> Option<Value> {
        if let Some(top) = self.stack.last_mut() {
            top.insert(v);
            None
        } else {
            Some(v)
        }
    }

    pub fn push_list(&mut self) {
        self.stack.push(Container::new_list());
    }

    pub fn push_dict(&mut self) {
        self.stack.push(Container::new_dict());
    }

    pub fn pop_container(&mut self) -> Option<Value> {
        let top = self.stack.pop()?;
        self.push_value(top.to_value())
    }
}

enum Container {
    List(ListBuilder),
    Dict(DictBuilder),
}

impl Container {
    fn new_list() -> Container {
        Container::List(ListBuilder::new())
    }

    fn new_dict() -> Container {
        Container::Dict(DictBuilder::new())
    }

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
