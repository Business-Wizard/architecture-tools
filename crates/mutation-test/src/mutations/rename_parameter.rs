use crate::model::{Candidate, MutationError};

const RENAME_PREFIX: &str = "awt_renamed_";

pub fn apply(source: &[u8], candidate: &Candidate) -> Result<Vec<u8>, MutationError> {
    let start = candidate.byte_start;
    let end = candidate.byte_end;

    if end > source.len() || start > end {
        return Err(MutationError::OutOfBounds);
    }

    let params_src = &source[start..end];
    let params_str = std::str::from_utf8(params_src).map_err(|_| MutationError::OutOfBounds)?;

    let new_params = rename_first_non_self(params_str).ok_or(MutationError::OutOfBounds)?;

    let mut result = Vec::with_capacity(source.len() + RENAME_PREFIX.len() + 32);
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(new_params.as_bytes());
    result.extend_from_slice(&source[end..]);

    Ok(result)
}

fn rename_first_non_self(params: &str) -> Option<String> {
    let inner = params.strip_prefix('(')?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();

    let first_non_self = parts.iter().position(|p| {
        let t = p.trim();
        t != "self" && t != "cls" && !t.is_empty()
    })?;

    let param = parts[first_non_self];
    let name = param.trim().split(':').next()?.split('=').next()?.trim();
    let renamed = param.replacen(name, &format!("{RENAME_PREFIX}{name}"), 1);

    let mut new_parts: Vec<String> = parts.iter().map(std::string::ToString::to_string).collect();
    new_parts[first_non_self] = renamed;

    Some(format!("({})", new_parts.join(",")))
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::model::{CandidateKind, MutantId, OperatorKind};

    fn make_candidate(byte_start: usize, byte_end: usize) -> Candidate {
        Candidate {
            id: MutantId::new("src/foo.py", "foo", "rename_parameter"),
            file: Utf8PathBuf::from("src/foo.py"),
            symbol: "foo".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::RenameParameter,
            line: 0,
            byte_start,
            byte_end,
        }
    }

    #[test]
    fn test_first_non_self_param_should_be_renamed() {
        let source = b"def foo(self, order_id: str): pass";
        let candidate = make_candidate(7, 28);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(self, awt_renamed_order_id: str): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_bare_function_first_param_should_be_renamed() {
        let source = b"def foo(x: int): pass";
        let candidate = make_candidate(7, 15);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(awt_renamed_x: int): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_out_of_bounds_byte_range_should_return_error() {
        let source = b"def foo(x: int): pass";
        let actual = apply(source, &make_candidate(7, 999));
        assert!(matches!(actual, Err(MutationError::OutOfBounds)));
    }

    #[test]
    fn test_cls_only_params_should_return_error() {
        let source = b"def foo(cls): pass";
        let actual = apply(source, &make_candidate(7, 12));
        assert!(matches!(actual, Err(MutationError::OutOfBounds)));
    }
}
