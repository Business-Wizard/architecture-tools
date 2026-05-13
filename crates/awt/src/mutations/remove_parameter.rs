use crate::model::{Candidate, MutationError};

pub fn apply(source: &[u8], candidate: &Candidate) -> Result<Vec<u8>, MutationError> {
    let start = candidate.byte_start;
    let end = candidate.byte_end;

    if end > source.len() || start > end {
        return Err(MutationError::OutOfBounds);
    }

    let params_src = &source[start..end];
    let params_str = std::str::from_utf8(params_src).map_err(|_| MutationError::OutOfBounds)?;

    let new_params = remove_last_param(params_str).ok_or(MutationError::OutOfBounds)?;

    let mut result = Vec::with_capacity(source.len());
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(new_params.as_bytes());
    result.extend_from_slice(&source[end..]);

    Ok(result)
}

fn remove_last_param(params: &str) -> Option<String> {
    let inner = params.strip_prefix('(')?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();

    // Find last non-self/cls param
    let last_removable = parts
        .iter()
        .enumerate()
        .rev()
        .find(|(_, p)| {
            let t = p.trim();
            t != "self" && t != "cls" && !t.is_empty()
        })
        .map(|(i, _)| i)?;

    let mut new_parts: Vec<&str> = parts.clone();
    new_parts.remove(last_removable);

    let joined = new_parts.join(",");
    Some(format!("({joined})"))
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::model::{CandidateKind, MutantId, OperatorKind};

    fn make_candidate(byte_start: usize, byte_end: usize) -> Candidate {
        Candidate {
            id: MutantId::new("src/foo.py", "foo", "remove_parameter"),
            file: Utf8PathBuf::from("src/foo.py"),
            symbol: "foo".into(),
            kind: CandidateKind::Function,
            operator: OperatorKind::RemoveParameter,
            line: 0,
            byte_start,
            byte_end,
        }
    }

    #[test]
    fn test_last_param_should_be_removed() {
        let source = b"def foo(x: int, y: str): pass";
        let candidate = make_candidate(7, 23);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(x: int): pass";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_self_with_one_param_should_remove_the_param() {
        let source = b"def foo(self, x: int): pass";
        let candidate = make_candidate(7, 21);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"def foo(self): pass";
        assert_eq!(actual, expected);
    }
}
