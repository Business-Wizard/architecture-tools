use std::fmt::Debug;

#[derive(Debug, PartialEq)]
pub enum ObjectType {
    Primitive,
    Enum,
    Function(Vec<ObjectType>),
    Struct,
    Class(Vec<ObjectType>),
    TraitLike,
    Interface,
}

#[derive(Debug, PartialEq)]
pub struct Abstractness(pub f32);

fn calculate_abstractness_ref(object_type: &ObjectType) -> f32 {
    match object_type {
        ObjectType::Primitive | ObjectType::Enum | ObjectType::Struct => 0.0,
        ObjectType::Function(params) | ObjectType::Class(params) => mean_abstractness(params),
        ObjectType::TraitLike | ObjectType::Interface => 1.0,
    }
}

fn mean_abstractness(params: &[ObjectType]) -> f32 {
    if params.is_empty() {
        return 0.0;
    }
    let sum: f32 = params.iter().map(calculate_abstractness_ref).sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = sum / params.len() as f32;
    mean
}

#[must_use]
pub fn calculate_abstractness(object_type: ObjectType) -> Abstractness {
    match object_type {
        ObjectType::Primitive | ObjectType::Enum | ObjectType::Struct => Abstractness(0.0),
        ObjectType::Function(params) | ObjectType::Class(params) => {
            Abstractness(mean_abstractness(&params))
        }
        ObjectType::TraitLike | ObjectType::Interface => Abstractness(1.0),
    }
}

#[cfg(test)]
mod test_abstractness {
    use super::*;

    #[test]
    fn given_an_enum_object_should_measure_zero() {
        let concrete_object = ObjectType::Enum;
        let actual = calculate_abstractness(concrete_object);
        let expected = Abstractness(0.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_an_trait_like_object_should_measure_one() {
        let abstract_object = ObjectType::TraitLike;
        let actual = calculate_abstractness(abstract_object);
        let expected = Abstractness(1.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_an_interface_object_should_measure_one() {
        let abstract_object = ObjectType::Interface;
        let actual = calculate_abstractness(abstract_object);
        let expected = Abstractness(1.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_no_params_should_measure_zero() {
        let actual = calculate_abstractness(ObjectType::Function(vec![]));
        let expected = Abstractness(0.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_only_primitive_params_should_measure_zero() {
        let actual = calculate_abstractness(ObjectType::Function(vec![
            ObjectType::Primitive,
            ObjectType::Primitive,
        ]));
        let expected = Abstractness(0.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_only_interface_params_should_measure_one() {
        let actual = calculate_abstractness(ObjectType::Function(vec![
            ObjectType::Interface,
            ObjectType::Interface,
        ]));
        let expected = Abstractness(1.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_function_with_mixed_primitive_and_interface_params_should_measure_half() {
        let actual = calculate_abstractness(ObjectType::Function(vec![
            ObjectType::Primitive,
            ObjectType::Interface,
        ]));
        let expected = Abstractness(0.5);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_with_no_params_should_measure_zero() {
        let actual = calculate_abstractness(ObjectType::Class(vec![]));
        let expected = Abstractness(0.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_with_only_primitive_params_should_measure_zero() {
        let actual = calculate_abstractness(ObjectType::Class(vec![ObjectType::Primitive]));
        let expected = Abstractness(0.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_with_only_interface_params_should_measure_one() {
        let actual = calculate_abstractness(ObjectType::Class(vec![ObjectType::Interface]));
        let expected = Abstractness(1.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn given_class_with_mixed_primitive_and_interface_params_should_measure_half() {
        let actual = calculate_abstractness(ObjectType::Class(vec![
            ObjectType::Primitive,
            ObjectType::Interface,
        ]));
        let expected = Abstractness(0.5);
        assert_eq!(actual, expected);
    }
}
