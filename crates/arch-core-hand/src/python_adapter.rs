use crate::domain::ObjectType;
use tree_sitter::{Node, Parser, Tree};

#[must_use]
pub fn parse_python(source: &str) -> Vec<ObjectType> {
    let parser = _construct_parser();
    let tree = _generate_ast(parser, source);

    _part_ast_into_domain(tree)
}

fn _construct_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    parser
}

fn _generate_ast(mut parser: Parser, source_code: &str) -> Tree {
    parser.parse(source_code, None).unwrap()
}

fn _part_ast_into_domain(tree: Tree) -> Vec<ObjectType> {
    let root = tree.root_node();
    let mut results = Vec::new();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        match node.kind() {
            "function_definition" => {
                let parameters = _extract_paramaters(node);
                results.push(ObjectType::Function(parameters));
            }
            "class_definition" => {
                results.push(ObjectType::Class);
            }
            _ => {}
        }
    }
    results
}

fn _extract_paramaters(node: Node) -> Vec<ObjectType> {
    let mut parameters: Vec<ObjectType> = Vec::new();
    let _parameter = node.child_by_field_name("parameters").unwrap();
    parameters.push(ObjectType::Primitive);
    parameters
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
        let expected = vec![ObjectType::Class];
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
        todo!()
    }

    #[test]
    fn given_class_init_with_const_param_should_return_class_type_with_const_param_detail() {
        todo!()
    }
}
