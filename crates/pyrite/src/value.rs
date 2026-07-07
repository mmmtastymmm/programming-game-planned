//! Runtime values. Deliberately small (docs/01-language.md "Types"):
//! ints, bools, strings (labels/channels), opaque entity handles, lists,
//! and enum values. **No floats** — determinism rule (CLAUDE.md).

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    /// Opaque handle to a world object; only the Host can interpret it.
    Entity(u64),
    List(Vec<Value>),
    Enum(EnumValue),
    /// The result of a function with no `return` value. Not constructible
    /// from source; using it in an operation is a type fault.
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    pub enum_name: String,
    pub variant: String,
    pub fields: Vec<Value>,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Entity(_) => "entity",
            Value::List(_) => "list",
            Value::Enum(_) => "enum",
            Value::Unit => "unit",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{}", if *v { "True" } else { "False" }),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Entity(id) => write!(f, "<entity {id}>"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Enum(e) => {
                write!(f, "{}.{}", e.enum_name, e.variant)?;
                if !e.fields.is_empty() {
                    write!(f, "(")?;
                    for (i, field) in e.fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{field}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Value::Unit => write!(f, "<unit>"),
        }
    }
}
