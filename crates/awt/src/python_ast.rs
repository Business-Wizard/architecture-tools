use tree_sitter::{Node, Parser, Tree};

use architecture_core::object_type::ObjectType;

pub struct ParsedFile {
    pub source: Vec<u8>,
    pub tree: Tree,
}

impl ParsedFile {
    pub fn parse(source: &[u8]) -> Option<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;
        Some(Self {
            source: source.to_vec(),
            tree,
        })
    }

    pub fn root(&self) -> Node<'_> {
        self.tree.root_node()
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct FunctionInfo {
    pub name: String,
    pub line: u32,
    pub params_byte_start: usize,
    pub params_byte_end: usize,
    pub is_method: bool,
    pub is_constructor: bool,
    pub has_varargs: bool,
    pub has_kwargs: bool,
    pub has_defaults: bool,
    pub has_overload: bool,
    pub has_property: bool,
    pub is_dunder: bool,
}

impl FunctionInfo {
    pub fn is_eligible_for_add_param(&self) -> bool {
        if self.has_varargs || self.has_kwargs || self.has_defaults {
            return false;
        }
        if self.has_overload || self.has_property {
            return false;
        }
        if self.is_dunder && !self.is_constructor {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module_path: String,
    pub line: u32,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Debug, PartialEq)]
pub enum ClassKind {
    Abstract,
    Protocol,
    Concrete,
}

#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub struct ClassInfo {
    pub kind: ClassKind,
}

pub fn find_functions(parsed: &ParsedFile) -> Vec<FunctionInfo> {
    let mut results = Vec::new();
    collect_functions(parsed.root(), &parsed.source, false, &mut results);
    results
}

fn collect_functions(node: Node<'_>, source: &[u8], in_class: bool, out: &mut Vec<FunctionInfo>) {
    let kind = node.kind();

    if kind == "function_definition"
        && let Some(info) = extract_function(node, source, in_class)
    {
        out.push(info);
    }

    let descend_into_class = kind == "class_definition";
    let now_in_class = in_class || descend_into_class;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, source, now_in_class, out);
    }
}

fn extract_function(node: Node<'_>, source: &[u8], in_class: bool) -> Option<FunctionInfo> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?.to_string();

    let params_node = node.child_by_field_name("parameters")?;
    let line = u32::try_from(node.start_position().row)
        .unwrap_or_else(|_| unreachable!("tree-sitter row exceeds u32::MAX"));
    let params_byte_start = params_node.start_byte();
    let params_byte_end = params_node.end_byte();

    let is_constructor = name == "__init__";
    let is_dunder = name.starts_with("__") && name.ends_with("__");

    let (has_varargs, has_kwargs, has_defaults) = inspect_params(params_node, source);

    let (has_overload, has_property) = inspect_decorators(node, source);

    let is_method = in_class;

    Some(FunctionInfo {
        name,
        line,
        params_byte_start,
        params_byte_end,
        is_method,
        is_constructor,
        has_varargs,
        has_kwargs,
        has_defaults,
        has_overload,
        has_property,
        is_dunder,
    })
}

fn inspect_params(params_node: Node<'_>, source: &[u8]) -> (bool, bool, bool) {
    let mut has_varargs = false;
    let mut has_kwargs = false;
    let mut has_defaults = false;

    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "list_splat_pattern" | "*" | "keyword_separator" => has_varargs = true,
            "dictionary_splat_pattern" | "**" => has_kwargs = true,
            "default_parameter" | "typed_default_parameter" => has_defaults = true,
            _ => {
                if let Ok(text) = child.utf8_text(source)
                    && text == "*"
                {
                    has_varargs = true;
                }
            }
        }
    }

    (has_varargs, has_kwargs, has_defaults)
}

fn inspect_decorators(func_node: Node<'_>, source: &[u8]) -> (bool, bool) {
    let mut has_overload = false;
    let mut has_property = false;

    let Some(parent) = func_node.parent() else {
        return (false, false);
    };

    let mut cursor = parent.walk();
    for sibling in parent.children(&mut cursor) {
        if sibling.kind() != "decorator" {
            continue;
        }
        let text = sibling.utf8_text(source).unwrap_or("");
        if text.contains("overload") {
            has_overload = true;
        }
        if text.contains("property") {
            has_property = true;
        }
    }

    (has_overload, has_property)
}

#[allow(dead_code)]
pub fn find_classes(parsed: &ParsedFile) -> Vec<ClassInfo> {
    let mut results = Vec::new();
    collect_classes(parsed.root(), &parsed.source, &mut results);
    results
}

#[allow(dead_code)]
fn collect_classes(node: Node<'_>, source: &[u8], out: &mut Vec<ClassInfo>) {
    if node.kind() == "class_definition" {
        out.push(extract_class(node, source));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_classes(child, source, out);
    }
}

#[allow(dead_code)]
fn extract_class(node: Node<'_>, source: &[u8]) -> ClassInfo {
    ClassInfo {
        kind: classify_class(node, source),
    }
}

#[allow(dead_code)]
fn classify_class(node: Node<'_>, source: &[u8]) -> ClassKind {
    let argument_list = find_argument_list(node);

    if let Some(arg_node) = argument_list {
        if has_base_with_name(arg_node, source, "Protocol") {
            return ClassKind::Protocol;
        }
        if has_base_with_name(arg_node, source, "ABC") {
            return ClassKind::Abstract;
        }
    }

    ClassKind::Concrete
}

#[allow(dead_code)]
fn find_argument_list(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "argument_list")
}

#[allow(dead_code)]
fn has_base_with_name(argument_list: Node<'_>, source: &[u8], target_name: &str) -> bool {
    let mut cursor = argument_list.walk();
    for child in argument_list.children(&mut cursor) {
        if let Ok(text) = child.utf8_text(source)
            && (text == target_name || text.ends_with(&format!(".{target_name}")))
        {
            return true;
        }
    }
    false
}

pub fn find_imports(parsed: &ParsedFile) -> Vec<ImportInfo> {
    let mut results = Vec::new();
    collect_imports(parsed.root(), &parsed.source, &mut results);
    results
}

pub fn extract_module_names(statement: &str) -> Vec<String> {
    let s = statement.trim();
    if let Some(rest) = s.strip_prefix("from ") {
        let module_part = rest.split_whitespace().next().unwrap_or("");
        if module_part.starts_with('.') {
            return vec![];
        }
        let module =
            module_part.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');
        if module.is_empty() {
            return vec![];
        }
        vec![module.to_string()]
    } else if let Some(rest) = s.strip_prefix("import ") {
        rest.split(',')
            .filter_map(|seg| {
                let name = seg.split_whitespace().next().unwrap_or("");
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
            .collect()
    } else {
        vec![]
    }
}

fn collect_imports(node: Node<'_>, source: &[u8], out: &mut Vec<ImportInfo>) {
    match node.kind() {
        "import_statement" | "import_from_statement" => {
            let text = node.utf8_text(source).unwrap_or("").to_string();
            out.push(ImportInfo {
                module_path: text,
                line: u32::try_from(node.start_position().row)
                    .unwrap_or_else(|_| unreachable!("tree-sitter row exceeds u32::MAX")),
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
            });
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_imports(child, source, out);
    }
}

#[must_use]
pub fn parse_objects(parsed: &ParsedFile) -> Vec<ObjectType> {
    part_ast_into_object_types(parsed.root(), &parsed.source)
}

fn part_ast_into_object_types(root: Node<'_>, source: &[u8]) -> Vec<ObjectType> {
    let mut results = Vec::new();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        match node.kind() {
            "function_definition" => {
                let parameters = extract_object_params(node, source);
                results.push(ObjectType::Function(parameters));
            }
            "class_definition" => {
                results.push(classify_python_class(node, source));
            }
            _ => {}
        }
    }
    results
}

fn classify_type_ref(node: Node<'_>, source: &[u8]) -> ObjectType {
    let type_node = node.child_by_field_name("type").unwrap_or(node);
    let text = type_node.utf8_text(source).unwrap_or("");
    match text {
        "Callable" | "Protocol" => ObjectType::Interface,
        "ABC" => ObjectType::TraitLike,
        _ => ObjectType::Primitive,
    }
}

fn extract_object_params(node: Node<'_>, source: &[u8]) -> Vec<ObjectType> {
    let mut parameters: Vec<ObjectType> = Vec::new();
    let params_node = node.child_by_field_name("parameters").unwrap();
    for i in 0..params_node.child_count() {
        let child = params_node.child(i).unwrap();
        match child.kind() {
            "identifier" if child.utf8_text(source).unwrap_or("") != "self" => {
                parameters.push(ObjectType::Primitive);
            }
            "typed_parameter" | "typed_default_parameter" => {
                parameters.push(classify_type_ref(child, source));
            }
            "default_parameter" => {
                parameters.push(ObjectType::Primitive);
            }
            _ => {}
        }
    }
    parameters
}

fn extract_python_superclasses(node: Node<'_>, source: &[u8]) -> Vec<String> {
    let Some(superclasses) = node.child_by_field_name("superclasses") else {
        return vec![];
    };
    let mut bases = Vec::new();
    for i in 0..superclasses.child_count() {
        let child = superclasses.child(i).unwrap();
        match child.kind() {
            "identifier" | "attribute" => {
                if let Ok(text) = child.utf8_text(source) {
                    bases.push(text.to_owned());
                }
            }
            _ => {}
        }
    }
    bases
}

fn extract_init_object_params(class_node: Node<'_>, source: &[u8]) -> Vec<ObjectType> {
    let Some(body) = class_node.child_by_field_name("body") else {
        return vec![];
    };
    for i in 0..body.child_count() {
        let child = body.child(i).unwrap();
        if child.kind() == "function_definition" {
            let name_node = child.child_by_field_name("name").unwrap();
            if name_node.utf8_text(source).unwrap_or("") == "__init__" {
                return extract_object_params(child, source);
            }
        }
    }
    vec![]
}

fn classify_python_class(node: Node<'_>, source: &[u8]) -> ObjectType {
    let bases = extract_python_superclasses(node, source);
    for base in &bases {
        match base.as_str() {
            "Protocol" | "typing.Protocol" => return ObjectType::Interface,
            "ABC" | "abc.ABC" => return ObjectType::TraitLike,
            "Enum" | "enum.Enum" => return ObjectType::Enum,
            _ => {}
        }
    }
    ObjectType::Class(extract_init_object_params(node, source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_module_names_import_single_should_return_module() {
        assert_eq!(extract_module_names("import foo"), vec!["foo"]);
    }

    #[test]
    fn test_extract_module_names_import_dotted_should_return_dotted_path() {
        assert_eq!(extract_module_names("import foo.bar"), vec!["foo.bar"]);
    }

    #[test]
    fn test_extract_module_names_import_multiple_should_return_all() {
        let mut actual = extract_module_names("import foo, bar");
        actual.sort();
        assert_eq!(actual, vec!["bar", "foo"]);
    }

    #[test]
    fn test_extract_module_names_import_with_alias_should_strip_alias() {
        assert_eq!(
            extract_module_names("import foo.bar as fb"),
            vec!["foo.bar"]
        );
    }

    #[test]
    fn test_extract_module_names_from_import_should_return_module() {
        assert_eq!(
            extract_module_names("from foo.bar import Baz"),
            vec!["foo.bar"]
        );
    }

    #[test]
    fn test_extract_module_names_relative_import_should_return_empty() {
        assert_eq!(
            extract_module_names("from . import sibling"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_extract_module_names_relative_dotted_should_return_empty() {
        assert_eq!(
            extract_module_names("from .sibling import X"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_plain_class_should_be_concrete() {
        let source = b"class Foo: pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Concrete,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_abc_base_should_be_abstract() {
        let source = b"class Foo(ABC): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Abstract,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_protocol_base_should_be_protocol() {
        let source = b"class Foo(Protocol): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Protocol,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_qualified_abc_base_should_be_abstract() {
        let source = b"class Foo(abc.ABC): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Abstract,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_qualified_protocol_base_should_be_protocol() {
        let source = b"class Foo(typing.Protocol): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Protocol,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_multiple_bases_with_protocol_should_be_protocol() {
        let source = b"class Foo(Base, Protocol): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![ClassInfo {
            kind: ClassKind::Protocol,
        }];
        assert_eq!(classes, expected);
    }

    #[test]
    fn test_multiple_classes_should_return_all() {
        let source = b"class Foo: pass\nclass Bar(ABC): pass";
        let parsed = ParsedFile::parse(source).expect("parse");
        let classes = find_classes(&parsed);
        let expected = vec![
            ClassInfo {
                kind: ClassKind::Concrete,
            },
            ClassInfo {
                kind: ClassKind::Abstract,
            },
        ];
        assert_eq!(classes, expected);
    }

    fn parse(src: &str) -> Vec<ObjectType> {
        let parsed = ParsedFile::parse(src.as_bytes()).unwrap();
        parse_objects(&parsed)
    }

    #[test]
    fn test_parse_objects_no_param_function_should_return_function_type() {
        assert_eq!(parse("def foo(): pass"), vec![ObjectType::Function(vec![])]);
    }

    #[test]
    fn test_parse_objects_class_should_return_class_type() {
        assert_eq!(parse("class Bar: pass"), vec![ObjectType::Class(vec![])]);
    }

    #[test]
    fn test_parse_objects_function_with_constant_param_should_return_function_type_with_const_param_detail()
     {
        assert_eq!(
            parse("def foo(parameter: int): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_func_param_should_return_function_type_with_func_param_detail()
     {
        assert_eq!(
            parse("def foo(handler: Callable): pass"),
            vec![ObjectType::Function(vec![ObjectType::Interface])]
        );
    }

    #[test]
    fn test_parse_objects_class_init_with_const_param_should_return_class_type_with_const_param_detail()
     {
        assert_eq!(
            parse("class Foo:\n    def __init__(self, x: int): pass"),
            vec![ObjectType::Class(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_untyped_param_should_return_function_with_primitive_param()
    {
        assert_eq!(
            parse("def foo(x): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_str_typed_param_should_return_function_with_primitive_param()
     {
        assert_eq!(
            parse("def foo(x: str): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_callable_typed_param_should_return_function_with_interface_param()
     {
        assert_eq!(
            parse("def foo(x: Callable): pass"),
            vec![ObjectType::Function(vec![ObjectType::Interface])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_protocol_typed_param_should_return_function_with_interface_param()
     {
        assert_eq!(
            parse("def foo(x: Protocol): pass"),
            vec![ObjectType::Function(vec![ObjectType::Interface])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_abc_typed_param_should_return_function_with_traitlike_param()
     {
        assert_eq!(
            parse("def foo(x: ABC): pass"),
            vec![ObjectType::Function(vec![ObjectType::TraitLike])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_unknown_typed_param_should_default_to_primitive() {
        assert_eq!(
            parse("def foo(x: MyClass): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_args_splat_should_skip_splat_and_return_preceding_params() {
        assert_eq!(
            parse("def foo(x: int, *args): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_kwargs_should_skip_kwargs_and_return_preceding_params() {
        assert_eq!(
            parse("def foo(x: str, **kwargs): pass"),
            vec![ObjectType::Function(vec![ObjectType::Primitive])]
        );
    }

    #[test]
    fn test_parse_objects_function_with_multiple_typed_params_should_return_all_classified() {
        assert_eq!(
            parse("def foo(x: int, y: Protocol): pass"),
            vec![ObjectType::Function(vec![
                ObjectType::Primitive,
                ObjectType::Interface
            ])]
        );
    }

    #[test]
    fn test_parse_objects_protocol_class_should_return_interface_type() {
        assert_eq!(
            parse("class Repo(Protocol): pass"),
            vec![ObjectType::Interface]
        );
    }

    #[test]
    fn test_parse_objects_typing_protocol_class_should_return_interface_type() {
        assert_eq!(
            parse("class Repo(typing.Protocol): pass"),
            vec![ObjectType::Interface]
        );
    }

    #[test]
    fn test_parse_objects_abc_class_should_return_traitlike_type() {
        assert_eq!(
            parse("class Service(ABC): pass"),
            vec![ObjectType::TraitLike]
        );
    }

    #[test]
    fn test_parse_objects_abc_abc_class_should_return_traitlike_type() {
        assert_eq!(
            parse("class Service(abc.ABC): pass"),
            vec![ObjectType::TraitLike]
        );
    }

    #[test]
    fn test_parse_objects_enum_class_should_return_enum_type() {
        assert_eq!(parse("class Color(Enum): pass"), vec![ObjectType::Enum]);
    }

    #[test]
    fn test_parse_objects_enum_enum_class_should_return_enum_type() {
        assert_eq!(
            parse("class Color(enum.Enum): pass"),
            vec![ObjectType::Enum]
        );
    }

    #[test]
    fn test_parse_objects_plain_class_inheriting_unknown_base_should_return_class_with_empty_params()
     {
        assert_eq!(
            parse("class Foo(Bar): pass"),
            vec![ObjectType::Class(vec![])]
        );
    }

    #[test]
    fn test_parse_objects_class_init_with_interface_param_should_return_class_with_interface_param()
    {
        assert_eq!(
            parse("class OrderService:\n    def __init__(self, repo: Protocol): pass"),
            vec![ObjectType::Class(vec![ObjectType::Interface])]
        );
    }

    #[test]
    fn test_parse_objects_class_init_with_mixed_params_should_return_class_with_blended_abstractness()
     {
        assert_eq!(
            parse("class BillingService:\n    def __init__(self, repo: Protocol, rate: int): pass"),
            vec![ObjectType::Class(vec![
                ObjectType::Interface,
                ObjectType::Primitive
            ])]
        );
    }

    #[test]
    fn test_parse_objects_class_init_with_only_self_should_return_class_with_empty_params() {
        assert_eq!(
            parse("class Foo:\n    def __init__(self): pass"),
            vec![ObjectType::Class(vec![])]
        );
    }

    #[test]
    fn test_parse_objects_class_with_no_init_should_return_class_with_empty_params() {
        assert_eq!(
            parse("class Foo:\n    def do_thing(self): pass"),
            vec![ObjectType::Class(vec![])]
        );
    }

    #[test]
    fn test_parse_objects_file_with_function_and_class_should_return_both() {
        assert_eq!(
            parse("def foo(): pass\nclass Bar: pass"),
            vec![ObjectType::Function(vec![]), ObjectType::Class(vec![])]
        );
    }

    #[test]
    fn test_parse_objects_file_with_protocol_and_concrete_class_should_classify_both() {
        assert_eq!(
            parse(
                "class Repo(Protocol): pass\nclass Service:\n    def __init__(self, repo: Protocol): pass"
            ),
            vec![
                ObjectType::Interface,
                ObjectType::Class(vec![ObjectType::Interface])
            ]
        );
    }
}
