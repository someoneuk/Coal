//! Embedded Luau VM. One instance is kept alive for the lifetime of the
//! injected library, so globals persist between executed scripts.

use mlua::{Lua, Value};

pub struct Vm {
    lua: Lua,
}

impl Vm {
    pub fn new() -> Result<Self, String> {
        let lua = Lua::new();
        lua.load("coal = { version = '0.1.0' }")
            .exec()
            .map_err(|e| e.to_string())?;
        Ok(Self { lua })
    }

    pub fn run(&self, source: &str) -> Result<String, String> {
        match self.lua.load(source).eval::<Value>() {
            Ok(value) => Ok(match value {
                Value::Nil => "(no return value)".to_string(),
                Value::Boolean(value) => value.to_string(),
                Value::Integer(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                Value::String(value) => value.to_string_lossy().to_owned(),
                other => format!("{other:?}"),
            }),
            Err(err) => Err(err.to_string()),
        }
    }
}
