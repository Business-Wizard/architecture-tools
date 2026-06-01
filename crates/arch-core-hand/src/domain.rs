use std::fmt::Debug;

#[derive(Debug, PartialEq)]
pub enum ObjectType {
    Primitive,
    Enum,
    Function(Vec<ObjectType>),
    Struct,
    Class,
    TraitLike,
    Interface,
}

#[derive(Debug, PartialEq)]
pub struct Abstractness(pub f32);

pub fn calculate_abstractness(object_type: ObjectType) -> Abstractness {
    match object_type {
        ObjectType::Primitive => Abstractness(0.0),
        ObjectType::Enum => Abstractness(0.0),
        ObjectType::Function(_) => Abstractness(0.0),
        ObjectType::Struct => Abstractness(0.0),
        ObjectType::Class => Abstractness(0.0),
        ObjectType::TraitLike => Abstractness(1.0),
        ObjectType::Interface => Abstractness(1.0),
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
}
