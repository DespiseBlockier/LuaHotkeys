use std::ops::Range;

use full_moon::{
    ast::{
        luau::{GenericDeclaration, TypeSpecifier},
        Block, Stmt,
    },
    node::Node,
    visitors::Visitor,
    LuaVersion,
};
use mlua::{Error, Function, Lua};

pub fn compile(lua: &Lua, source: &str, name: &str) -> mlua::Result<Function> {
    let name = format!("@{name}");
    let compile = |text: &str| lua.load(text).set_name(&name).into_function();
    match compile(source) {
        Err(error @ Error::SyntaxError { .. }) => match erase(source) {
            Some(lua_source) => compile(&lua_source),
            None => Err(error),
        },
        result => result,
    }
}

#[derive(Default)]
struct Types(Vec<Range<usize>>);

impl Types {
    fn remove(&mut self, node: &impl Node) {
        // The delimiter tokens of a table or generic type can extend past Node::range().
        let mut tokens = node.tokens();
        let Some(first) = tokens.next() else {
            return;
        };
        let mut start = first.token().start_position().bytes();
        let mut end = first.token().end_position().bytes();
        for token in tokens {
            start = start.min(token.token().start_position().bytes());
            end = end.max(token.token().end_position().bytes());
        }
        self.0.push(start..end);
    }
}

impl Visitor for Types {
    fn visit_block(&mut self, block: &Block) {
        for (statement, semicolon) in block.stmts_with_semicolon() {
            if matches!(
                statement,
                Stmt::TypeDeclaration(_) | Stmt::ExportedTypeDeclaration(_)
            ) {
                self.remove(statement);
                if let Some(semicolon) = semicolon {
                    self.remove(semicolon);
                }
            }
        }
    }

    fn visit_type_specifier(&mut self, specifier: &TypeSpecifier) {
        self.remove(specifier);
    }

    fn visit_generic_declaration(&mut self, generics: &GenericDeclaration) {
        self.remove(generics);
    }
}

fn erase(source: &str) -> Option<String> {
    let result = full_moon::parse_fallible(source, LuaVersion::luau());
    if !result.errors().is_empty() {
        return None;
    }
    let mut types = Types::default();
    types.visit_ast(result.ast());
    if types.0.is_empty() {
        return None;
    }

    // Blank type syntax without shifting Lua's line numbers or joining adjacent tokens.
    let mut bytes = source.as_bytes().to_vec();
    for range in types.0 {
        for byte in bytes.get_mut(range)? {
            if !matches!(*byte, b'\r' | b'\n') {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_annotated_lua_with_custom_types_and_preserves_lua_calls() {
        let lua = Lua::new();
        let source = r#"
-- Keep Unicode preceding type declarations and ordinary method calls intact: é 😀
type Key = Combo;
export type Point = {
    x: number,
    y: number,
}
type Box<T> = { value: T }
local function identity<T>(value: T): T
    return value
end
local shortcut: Key, point: Point = "Ctrl+Alt+J", { x = 3, y = 5 }
local wrapped: Box<string> = { value = shortcut }
return identity(wrapped.value), point.x + point.y, ("é: Point"):upper(), type(point)
"#;
        let script = compile(&lua, source, "types.lhk").unwrap();
        let values: (String, i32, String, String) = script.call(()).unwrap();
        assert_eq!(
            values,
            ("Ctrl+Alt+J".into(), 8, "é: POINT".into(), "table".into())
        );
        let plain = compile(&lua, r#"return ("plain"):upper()"#, "plain.lua").unwrap();
        assert_eq!(plain.call::<String>(()).unwrap(), "PLAIN");
    }

    #[test]
    fn typed_scripts_report_the_original_line_number() {
        let lua = Lua::new();
        let source = "type T = {\r\n value: string\r\n}\r\nlocal value: T = {}\r\nerror('boom')";
        let script = compile(&lua, source, "test.lhk").unwrap();
        let error = script.call::<()>(()).unwrap_err().to_string();
        assert!(error.contains("test.lhk:5: boom"), "{error}");
    }
}
