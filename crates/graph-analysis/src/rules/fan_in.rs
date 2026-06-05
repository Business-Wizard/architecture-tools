use std::collections::HashMap;

use py_analyzer::InspectResult;

use crate::model::{GraphRuleId, GraphSeverity, GraphViolation, ViolationKind};

pub fn check(result: &InspectResult, threshold: usize) -> Vec<GraphViolation> {
    if threshold == 0 {
        return vec![];
    }

    let mut fan_in: HashMap<String, usize> = HashMap::new();
    for cls in &result.classes {
        for dep in &cls.class_deps {
            *fan_in.entry(dep.clone()).or_insert(0) += 1;
        }
    }

    fan_in
        .into_iter()
        .filter(|(_, count)| *count > threshold)
        .map(|(class, count)| GraphViolation {
            rule: GraphRuleId::HighFanIn,
            severity: GraphSeverity::Warning,
            message: format!("{class}  (fan-in: {count}, threshold: {threshold})"),
            kind: ViolationKind::HighFanIn {
                class,
                fan_in: count,
                threshold,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use py_analyzer::{ClassDef, InspectResult};

    fn make_class(module: &str, name: &str, deps: &[&str]) -> ClassDef {
        ClassDef {
            module: module.to_string(),
            name: name.to_string(),
            bases: vec![],
            attributes: vec![],
            methods: vec![],
            class_deps: deps.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn make_result(classes: Vec<ClassDef>) -> InspectResult {
        InspectResult {
            module_deps: vec![],
            classes,
        }
    }

    #[test]
    fn test_fan_in_above_threshold_should_produce_violation() {
        let classes: Vec<ClassDef> = (0..9)
            .map(|i| make_class("mod", &format!("User{i}"), &["domain.Error"]))
            .collect();
        let actual = check(&make_result(classes), 8);
        assert_eq!(actual.len(), 1);
        assert!(matches!(
            actual[0].kind,
            ViolationKind::HighFanIn { fan_in: 9, .. }
        ));
    }

    #[test]
    fn test_fan_in_at_threshold_should_not_produce_violation() {
        let classes: Vec<ClassDef> = (0..8)
            .map(|i| make_class("mod", &format!("User{i}"), &["domain.Error"]))
            .collect();
        let actual = check(&make_result(classes), 8);
        assert_eq!(actual, vec![]);
    }

    #[test]
    fn test_fan_in_multiple_magnets_should_produce_one_violation_each() {
        let mut classes: Vec<ClassDef> = (0..9)
            .map(|i| make_class("mod", &format!("A{i}"), &["domain.ErrorA"]))
            .collect();
        classes.extend((0..10).map(|i| make_class("mod", &format!("B{i}"), &["domain.ErrorB"])));
        let actual = check(&make_result(classes), 8);
        assert_eq!(actual.len(), 2);
    }
}
