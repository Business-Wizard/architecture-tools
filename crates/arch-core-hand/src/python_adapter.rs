use crate::domain::*;
use tree_sitter::*;

pub fn parse_python(source: &str) -> Vec<ObjectType> {
    let parser = _construct_parser();
    let tree = _generate_ast(parser, source);
    let results = _part_ast_into_domain(tree);
    results
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
                results.push(ObjectType::Function);
            }
            "class_definition" => {
                results.push(ObjectType::Class);
            }
            _ => {}
        }
    }
    results
}

#[cfg(test)]
mod test_python_parser {
    use super::*;

    #[test]
    fn given_no_param_function_should_return_function_type() {
        let stub_source_code = "def foo(): pass";
        let actual = parse_python(stub_source_code);
        let expected = vec![ObjectType::Function];
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
        todo!()
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
