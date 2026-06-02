use crate::domain::ObjectType;
use tree_sitter::{Node, Parser, Tree};

#[must_use]
pub fn parse_python(source: &str) -> Vec<ObjectType> {
    let parser = construct_parser();
    let tree = generate_ast(parser, source);

    part_ast_into_domain(&tree, source.as_bytes())
}

fn construct_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    parser
}

fn generate_ast(mut parser: Parser, source_code: &str) -> Tree {
    parser.parse(source_code, None).unwrap()
}

fn part_ast_into_domain(tree: &Tree, source: &[u8]) -> Vec<ObjectType> {
    let root = tree.root_node();
    let mut results = Vec::new();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        match node.kind() {
            "function_definition" => {
                let parameters = extract_paramaters(node, source);
                results.push(ObjectType::Function(parameters));
            }
            "class_definition" => {
                results.push(classify_class(node, source));
            }
            _ => {}
        }
    }
    results
}

fn classify_type_annotation(node: Node, source: &[u8]) -> ObjectType {
    let type_node = node.child_by_field_name("type").unwrap_or(node);
    let text = type_node.utf8_text(source).unwrap_or("");
    match text {
        "Callable" | "Protocol" => ObjectType::Interface,
        "ABC" => ObjectType::TraitLike,
        _ => ObjectType::Primitive,
    }
}

fn extract_paramaters(node: Node, source: &[u8]) -> Vec<ObjectType> {
    let mut parameters: Vec<ObjectType> = Vec::new();
    let params_node = node.child_by_field_name("parameters").unwrap();
    for i in 0..params_node.child_count() {
        let child = params_node.child(i).unwrap();
        match child.kind() {
            "identifier" if child.utf8_text(source).unwrap_or("") != "self" => {
                parameters.push(ObjectType::Primitive);
            }
            "typed_parameter" | "typed_default_parameter" => {
                parameters.push(classify_type_annotation(child, source));
            }
            "default_parameter" => {
                parameters.push(ObjectType::Primitive);
            }
            _ => {}
        }
    }
    parameters
}

fn extract_superclasses(node: Node, source: &[u8]) -> Vec<String> {
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

fn extract_init_params(class_node: Node, source: &[u8]) -> Vec<ObjectType> {
    let Some(body) = class_node.child_by_field_name("body") else {
        return vec![];
    };
    for i in 0..body.child_count() {
        let child = body.child(i).unwrap();
        if child.kind() == "function_definition" {
            let name_node = child.child_by_field_name("name").unwrap();
            if name_node.utf8_text(source).unwrap_or("") == "__init__" {
                return extract_paramaters(child, source);
            }
        }
    }
    vec![]
}

fn classify_class(node: Node, source: &[u8]) -> ObjectType {
    let bases = extract_superclasses(node, source);
    for base in &bases {
        match base.as_str() {
            "Protocol" | "typing.Protocol" => return ObjectType::Interface,
            "ABC" | "abc.ABC" => return ObjectType::TraitLike,
            "Enum" | "enum.Enum" => return ObjectType::Enum,
            _ => {}
        }
    }
    ObjectType::Class(extract_init_params(node, source))
}

#[cfg(test)]
mod test_python_parser {
    use super::*;

    #[test]
    fn given_no_param_function_should_return_function_type() {
        let stub_source_code = "def foo(): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_should_return_class_type() {
        let stub_class = "class Bar: pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_constant_param_should_return_function_type_with_const_param_detail() {
        let stub_source_code = "def foo(parameter: int): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_func_param_should_return_function_type_with_func_param_detail() {
        let stub_source_code = "def foo(handler: Callable): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Interface])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_init_with_const_param_should_return_class_type_with_const_param_detail() {
        let stub_class = "class Foo:\n    def __init__(self, x: int): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_untyped_param_should_return_function_with_primitive_param() {
        let stub_source_code = "def foo(x): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_str_typed_param_should_return_function_with_primitive_param() {
        let stub_source_code = "def foo(x: str): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_float_typed_param_should_return_function_with_primitive_param() {
        let stub_source_code = "def foo(x: float): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_bool_typed_param_should_return_function_with_primitive_param() {
        let stub_source_code = "def foo(x: bool): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_callable_typed_param_should_return_function_with_interface_param() {
        let stub_source_code = "def foo(x: Callable): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Interface])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_protocol_typed_param_should_return_function_with_interface_param() {
        let stub_source_code = "def foo(x: Protocol): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Interface])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_abc_typed_param_should_return_function_with_traitlike_param() {
        let stub_source_code = "def foo(x: ABC): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::TraitLike])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_unknown_typed_param_should_default_to_primitive() {
        let stub_source_code = "def foo(x: MyClass): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_args_splat_should_skip_splat_and_return_preceding_params() {
        let stub_source_code = "def foo(x: int, *args): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_kwargs_should_skip_kwargs_and_return_preceding_params() {
        let stub_source_code = "def foo(x: str, **kwargs): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_multiple_typed_params_should_return_all_classified() {
        let stub_source_code = "def foo(x: int, y: Protocol): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![
            ObjectType::Primitive,
            ObjectType::Interface,
        ])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_default_param_should_return_function_with_primitive_param() {
        let stub_source_code = "def foo(x=None): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function(vec![ObjectType::Primitive])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_protocol_class_should_return_interface_type() {
        let stub_class = "class Repo(Protocol): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Interface];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_typing_protocol_class_should_return_interface_type() {
        let stub_class = "class Repo(typing.Protocol): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Interface];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_abc_class_should_return_traitlike_type() {
        let stub_class = "class Service(ABC): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::TraitLike];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_abc_abc_class_should_return_traitlike_type() {
        let stub_class = "class Service(abc.ABC): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::TraitLike];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_enum_class_should_return_enum_type() {
        let stub_class = "class Color(Enum): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Enum];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_enum_enum_class_should_return_enum_type() {
        let stub_class = "class Color(enum.Enum): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Enum];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_plain_class_inheriting_unknown_base_should_return_class_with_empty_params() {
        let stub_class = "class Foo(Bar): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_init_with_interface_param_should_return_class_with_interface_param() {
        let stub_class = "class OrderService:\n    def __init__(self, repo: Protocol): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![ObjectType::Interface])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_init_with_mixed_params_should_return_class_with_blended_abstractness() {
        let stub_class =
            "class BillingService:\n    def __init__(self, repo: Protocol, rate: int): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![
            ObjectType::Interface,
            ObjectType::Primitive,
        ])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_init_with_only_self_should_return_class_with_empty_params() {
        let stub_class = "class Foo:\n    def __init__(self): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_with_no_init_should_return_class_with_empty_params() {
        let stub_class = "class Foo:\n    def do_thing(self): pass";
        let actual = parse_python(stub_class);
        let expected = vec![ObjectType::Class(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_file_with_function_and_class_should_return_both() {
        let stub_source = "def foo(): pass\nclass Bar: pass";
        let actual = parse_python(stub_source);
        let expected = vec![ObjectType::Function(vec![]), ObjectType::Class(vec![])];
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_file_with_protocol_and_concrete_class_should_classify_both() {
        let stub_source = "class Repo(Protocol): pass\nclass Service:\n    def __init__(self, repo: Protocol): pass";
        let actual = parse_python(stub_source);
        let expected = vec![
            ObjectType::Interface,
            ObjectType::Class(vec![ObjectType::Interface]),
        ];
        assert_eq!(actual, expected);
    }
}
