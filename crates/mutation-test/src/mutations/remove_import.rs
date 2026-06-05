use crate::model::{Candidate, MutationError};

/// # Errors
/// Returns `MutationError::OutOfBounds` if the byte range exceeds the source length.
pub fn apply(source: &[u8], candidate: &Candidate) -> Result<Vec<u8>, MutationError> {
    let start = candidate.byte_start;
    let end = candidate.byte_end;

    if end > source.len() || start > end {
        return Err(MutationError::OutOfBounds);
    }

    let replacement = b"# import removed by awt\n";
    let mut result = Vec::with_capacity(source.len());
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(replacement);
    result.extend_from_slice(&source[end..]);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;
    use crate::model::{CandidateKind, MutantId, OperatorKind};

    fn make_candidate(byte_start: usize, byte_end: usize) -> Candidate {
        Candidate {
            id: MutantId::new("src/foo.py", "from bar import baz", "remove_import"),
            file: Utf8PathBuf::from("src/foo.py"),
            symbol: "from bar import baz".into(),
            kind: CandidateKind::Import,
            operator: OperatorKind::RemoveImport,
            line: 0,
            byte_start,
            byte_end,
        }
    }

    #[test]
    fn test_import_line_should_be_replaced_with_comment() {
        let source = b"from bar import baz\nx = baz()\n";
        let candidate = make_candidate(0, 19);
        let actual = apply(source, &candidate).unwrap();
        let expected = b"# import removed by awt\n\nx = baz()\n";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_out_of_bounds_byte_range_should_return_error() {
        let source = b"import os\n";
        let actual = apply(source, &make_candidate(0, 999));
        assert!(matches!(actual, Err(MutationError::OutOfBounds)));
    }
}
