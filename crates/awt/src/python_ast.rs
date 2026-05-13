use tree_sitter::{Node, Parser, Tree};

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

pub fn find_functions(parsed: &ParsedFile) -> Vec<FunctionInfo> {
    let mut results = Vec::new();
    collect_functions(parsed.root(), &parsed.source, false, &mut results);
    results
}

fn collect_functions(node: Node<'_>, source: &[u8], in_class: bool, out: &mut Vec<FunctionInfo>) {
    let kind = node.kind();

    if kind == "function_definition" {
        if let Some(info) = extract_function(node, source, in_class) {
            out.push(info);
        }
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
    let line = node.start_position().row as u32;
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
            "list_splat_pattern" | "*" => has_varargs = true,
            "dictionary_splat_pattern" | "**" => has_kwargs = true,
            "default_parameter" | "typed_default_parameter" => has_defaults = true,
            "keyword_separator" => has_varargs = true,
            _ => {
                if let Ok(text) = child.utf8_text(source) {
                    if text == "*" {
                        has_varargs = true;
                    }
                }
            }
        }
    }

    (has_varargs, has_kwargs, has_defaults)
}

fn inspect_decorators(func_node: Node<'_>, source: &[u8]) -> (bool, bool) {
    let mut has_overload = false;
    let mut has_property = false;

    let parent = match func_node.parent() {
        Some(p) => p,
        None => return (false, false),
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

pub fn find_imports(parsed: &ParsedFile) -> Vec<ImportInfo> {
    let mut results = Vec::new();
    collect_imports(parsed.root(), &parsed.source, &mut results);
    results
}

fn collect_imports(node: Node<'_>, source: &[u8], out: &mut Vec<ImportInfo>) {
    match node.kind() {
        "import_statement" | "import_from_statement" => {
            let text = node.utf8_text(source).unwrap_or("").to_string();
            out.push(ImportInfo {
                module_path: text,
                line: node.start_position().row as u32,
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
