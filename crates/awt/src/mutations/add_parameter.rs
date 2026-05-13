use crate::model::{Candidate, MutationError};

const PROBE_PARAM: &str = "awt_required_probe: object";

pub fn apply(source: &[u8], candidate: &Candidate) -> Result<Vec<u8>, MutationError> {
    let start = candidate.byte_start;
    let end = candidate.byte_end;

    if end > source.len() || start > end {
        return Err(MutationError::OutOfBounds);
    }

    let params_src = &source[start..end];

    // Find the closing ')' inside the params slice.
    let close_offset = find_closing_paren(params_src).ok_or(MutationError::OutOfBounds)?;
    let insert_at = start + close_offset;

    let insertion = build_insertion(params_src, close_offset);

    let mut result = Vec::with_capacity(source.len() + insertion.len());
    result.extend_from_slice(&source[..insert_at]);
    result.extend_from_slice(insertion.as_bytes());
    result.extend_from_slice(&source[insert_at..]);

    Ok(result)
}

fn find_closing_paren(params: &[u8]) -> Option<usize> {
    params.iter().rposition(|&b| b == b')')
}

fn build_insertion(params_src: &[u8], close_offset: usize) -> String {
    let before_close = &params_src[..close_offset];
    let trimmed = before_close
        .iter()
        .rev()
        .find(|&&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r');

    // If the param list has only `self`/`cls` or is empty, insert without leading comma.
    // Otherwise prepend a comma separator.
    let needs_comma = trimmed.is_some_and(|&b| b != b'(');

    if needs_comma {
        format!(", {PROBE_PARAM}")
    } else {
        PROBE_PARAM.to_string()
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::model::{CandidateKind, MutantId, OperatorKind};

    fn make_candidate(byte_start: usize, byte_end: usize) -> Candidate {
        Candidate {
            id: MutantId::new("src/foo.py", "my_func", "add_required_parameter"),
            file: Utf8PathBuf::from("src/foo.py"),
            symbol: "my_func".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::AddRequiredParameter,
            line: 0,
            byte_start,
            byte_end,
        }
    }

    #[test]
    fn test_empty_params_should_insert_probe_without_comma() {
        let source = b"def foo(): pass";
        let candidate = make_candidate(7, 9);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(awt_required_probe: object): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_self_only_params_should_insert_probe_with_comma() {
        let source = b"def foo(self): pass";
        let candidate = make_candidate(7, 13);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(self, awt_required_probe: object): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_typed_params_should_insert_probe_with_comma() {
        let source = b"def foo(x: int, y: str): pass";
        let candidate = make_candidate(7, 23);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(x: int, y: str, awt_required_probe: object): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_out_of_bounds_byte_range_should_return_error() {
        let source = b"def foo(): pass";
        let candidate = make_candidate(7, 999);
        let actual = apply(source, &candidate);
        assert!(matches!(actual, Err(MutationError::OutOfBounds)));
    }
}
