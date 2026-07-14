//! Runtime values. Deliberately small (docs/01-language.md "Types"):
//! ints, bools, strings (labels/channels), opaque entity handles, lists,
//! dicts, and enum values. **No floats** — determinism rule (CLAUDE.md).
//!
//! Containers are **values, not references**: assignment copies, and
//! mutation happens only through a named variable (`xs.append(v)`,
//! `d[k] = v`). No aliasing — simpler to reason about, and deterministic
//! by construction.

use std::collections::BTreeMap;
use std::fmt;

/// A dict key: the orderable value subset, so dict storage/iteration is a
/// `BTreeMap` walk — deterministic by construction (CLAUDE.md rule 3;
/// iteration order is sorted, not insertion). Entities are valid keys on
/// purpose: per-target state (`seen[enemy] = tick`) is the headline use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DictKey {
    Int(i64),
    Str(String),
    Entity(u64),
}

impl DictKey {
    pub fn from_value(v: Value) -> Result<DictKey, String> {
        match v {
            Value::Int(i) => Ok(DictKey::Int(i)),
            Value::Str(s) => Ok(DictKey::Str(s)),
            Value::Entity(id) => Ok(DictKey::Entity(id)),
            other => Err(format!("dict keys must be int, string, or entity, got {}", other.type_name())),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            DictKey::Int(i) => Value::Int(*i),
            DictKey::Str(s) => Value::Str(s.clone()),
            DictKey::Entity(id) => Value::Entity(*id),
        }
    }
}

impl fmt::Display for DictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_value())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    /// Opaque handle to a world object; only the Host can interpret it.
    Entity(u64),
    List(Vec<Value>),
    /// Sorted-key map (iteration order = key order, never insertion).
    Dict(BTreeMap<DictKey, Value>),
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
    /// Name of the builtin fallible-query enum: `Result.Ok(v)` / `Result.Err(msg)`.
    /// Constructed by hosts, consumed by `.expect()` or `match`.
    pub const RESULT_ENUM: &'static str = "Result";

    pub fn result_ok(value: Value) -> Value {
        Value::Enum(EnumValue {
            enum_name: Self::RESULT_ENUM.to_string(),
            variant: "Ok".to_string(),
            fields: vec![value],
        })
    }

    pub fn result_err(msg: impl Into<String>) -> Value {
        Value::Enum(EnumValue {
            enum_name: Self::RESULT_ENUM.to_string(),
            variant: "Err".to_string(),
            fields: vec![Value::Str(msg.into())],
        })
    }

    /// Name of the builtin optional enum: `Option.Some(v)` / `Option.None`
    /// (docs/01 "None is an enum, not a null"). `dict.get(k)` returns it.
    pub const OPTION_ENUM: &'static str = "Option";

    pub fn option_some(value: Value) -> Value {
        Value::Enum(EnumValue {
            enum_name: Self::OPTION_ENUM.to_string(),
            variant: "Some".to_string(),
            fields: vec![value],
        })
    }

    pub fn option_none() -> Value {
        Value::Enum(EnumValue {
            enum_name: Self::OPTION_ENUM.to_string(),
            variant: "None".to_string(),
            fields: Vec::new(),
        })
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Entity(_) => "entity",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
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
            Value::Dict(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
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
