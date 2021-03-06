use super::checker::*;
use crate::lexers::{
    assignments::{Key, KeyIterator, Operator, Primitive, TypeError},
    ArgumentSplitter,
};
use std::fmt::{self, Display, Formatter};

#[derive(Debug, PartialEq)]
pub(crate) enum AssignmentError<'a> {
    InvalidValue(Primitive, Primitive),
    TypeError(TypeError),
    ExtraValues(&'a str, &'a str),
    ExtraKeys(&'a str, &'a str),
    RepeatedKey(&'a str),
    NoKey(&'a str),
}

impl<'a> Display for AssignmentError<'a> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            AssignmentError::InvalidValue(ref expected, ref actual) => {
                write!(f, "expected {}, but received {}", expected, actual)
            }
            AssignmentError::TypeError(ref type_err) => write!(f, "{}", type_err),
            AssignmentError::ExtraValues(ref prevkey, ref prevval) => write!(
                f,
                "extra values were supplied, and thus ignored. Previous assignment: '{}' = '{}'",
                prevkey, prevval
            ),
            AssignmentError::ExtraKeys(ref prevkey, ref prevval) => write!(
                f,
                "extra keys were supplied, and thus ignored. Previous assignment: '{}' = '{}'",
                prevkey, prevval
            ),
            AssignmentError::RepeatedKey(ref repkey) => write!(
                f,
                "repeated assignment to same key, and thus ignored. Repeated key: '{}'",
                repkey
            ),
            AssignmentError::NoKey(ref lone_val) => {
                write!(f, "no key to assign value, thus ignored. Value: '{}'", lone_val)
            }
        }
    }
}

/// An iterator structure which returns `Action` enums which tell the shell how to enact the
/// assignment request.
///
/// Each request will tell the shell whether the assignment is asking to update an array or a
/// string, and will contain the key/value pair to assign.
#[derive(Debug)]
pub(crate) struct AssignmentActions<'a> {
    keys:     KeyIterator<'a>,
    operator: Operator,
    values:   ArgumentSplitter<'a>,
    prevkeys: Vec<&'a str>,
    prevval:  &'a str,
}

impl<'a> AssignmentActions<'a> {
    pub(crate) fn new(keys: &'a str, operator: Operator, values: &'a str) -> AssignmentActions<'a> {
        AssignmentActions {
            keys: KeyIterator::new(keys),
            operator,
            values: ArgumentSplitter::new(values),
            prevkeys: Vec::new(),
            prevval: "",
        }
    }
}

impl<'a> Iterator for AssignmentActions<'a> {
    type Item = Result<Action<'a>, AssignmentError<'a>>;

    fn next(&mut self) -> Option<Result<Action<'a>, AssignmentError<'a>>> {
        match (self.keys.next(), self.values.next()) {
            (Some(key), Some(value)) => match key {
                Ok(key) => {
                    if self.prevkeys.contains(&key.name) {
                        Some(Err(AssignmentError::RepeatedKey(key.name)))
                    } else {
                        self.prevkeys.push(key.name);
                        self.prevval = value;
                        Some(Action::parse(key, self.operator, value, is_array(value)))
                    }
                }
                Err(why) => Some(Err(AssignmentError::TypeError(why))),
            },
            (None, Some(lone_val)) => {
                if let Some(&prevkey) = self.prevkeys.last() {
                    Some(Err(AssignmentError::ExtraValues(prevkey, self.prevval)))
                } else {
                    Some(Err(AssignmentError::NoKey(lone_val)))
                }
            }
            (Some(_), None) => {
                if let Some(&prevkey) = self.prevkeys.last() {
                    Some(Err(AssignmentError::ExtraKeys(prevkey, self.prevval)))
                } else {
                    unreachable!()
                }
            }
            _ => None,
        }
    }
}

/// Defines which type of assignment action is to be performed.
///
/// Providing the key/value pair and operator to use during assignment, this variant defines
/// whether the assignment should set a string or array.
#[derive(Debug, PartialEq)]
pub(crate) struct Action<'a>(pub Key<'a>, pub Operator, pub &'a str);

impl<'a> Action<'a> {
    fn parse(
        var: Key<'a>,
        operator: Operator,
        value: &'a str,
        is_array: bool,
    ) -> Result<Action<'a>, AssignmentError<'a>> {
        match var.kind {
            Primitive::Indexed(..) | Primitive::Str => Ok(Action(var, operator, value)),
            Primitive::StrArray
            | Primitive::BooleanArray
            | Primitive::FloatArray
            | Primitive::IntegerArray
            | Primitive::HashMap(_)
            | Primitive::BTreeMap(_) => {
                if is_array {
                    Ok(Action(var, operator, value))
                } else {
                    Err(AssignmentError::InvalidValue(var.kind, Primitive::Str))
                }
            }
            _ if !is_array => Ok(Action(var, operator, value)),
            _ => Err(AssignmentError::InvalidValue(var.kind, Primitive::StrArray)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexers::assignments::*;

    fn split(input: &str) -> (String, Operator, String) {
        let (keys, op, vals) = assignment_lexer(input);
        (keys.unwrap().into(), op.unwrap(), vals.unwrap().into())
    }

    #[test]
    fn assignment_actions() {
        let (keys, op, vals) = split("abc def = 123 456");
        let actions = AssignmentActions::new(&keys, op, &vals).collect::<Vec<_>>();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0],
            Ok(Action(Key { name: "abc", kind: Primitive::Str }, Operator::Equal, "123",))
        );
        assert_eq!(
            actions[1],
            Ok(Action(Key { name: "def", kind: Primitive::Str }, Operator::Equal, "456",))
        );

        let (keys, op, vals) = split("ab:int *= 3");
        let actions = AssignmentActions::new(&keys, op, &vals).collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            Ok(Action(Key { name: "ab", kind: Primitive::Integer }, Operator::Multiply, "3",))
        );

        let (keys, op, vals) = split("a b[] c:[int] = one [two three] [4 5 6]");
        let actions = AssignmentActions::new(&keys, op, &vals).collect::<Vec<_>>();
        assert_eq!(actions.len(), 3);
        assert_eq!(
            actions[0],
            Ok(Action(Key { name: "a", kind: Primitive::Str }, Operator::Equal, "one",))
        );
        assert_eq!(
            actions[1],
            Ok(Action(
                Key { name: "b", kind: Primitive::StrArray },
                Operator::Equal,
                "[two three]",
            ))
        );
        assert_eq!(
            actions[2],
            Ok(Action(
                Key { name: "c", kind: Primitive::IntegerArray },
                Operator::Equal,
                "[4 5 6]",
            ))
        );

        let (keys, op, values) = split("a[] b c[] = [one two] three [four five]");
        let actions = AssignmentActions::new(&keys, op, &values).collect::<Vec<_>>();
        assert_eq!(actions.len(), 3);
        assert_eq!(
            actions[0],
            Ok(Action(Key { name: "a", kind: Primitive::StrArray }, Operator::Equal, "[one two]",))
        );
        assert_eq!(
            actions[1],
            Ok(Action(Key { name: "b", kind: Primitive::Str }, Operator::Equal, "three",))
        );
        assert_eq!(
            actions[2],
            Ok(Action(
                Key { name: "c", kind: Primitive::StrArray },
                Operator::Equal,
                "[four five]",
            ))
        );
        let (keys, op, values) = split("array ++= [one two three four five]");
        let actions = AssignmentActions::new(&keys, op, &values).collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            Ok(Action(
                Key { name: "array", kind: Primitive::Str },
                Operator::Concatenate,
                "[one two three four five]",
            ))
        );
        let (keys, op, values) = split("array ::= [1 2 3 4 5]");
        let actions = AssignmentActions::new(&keys, op, &values).collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            Ok(Action(
                Key { name: "array", kind: Primitive::Str },
                Operator::ConcatenateHead,
                "[1 2 3 4 5]",
            ))
        );
        let (keys, op, values) = split(r"array \\= [foo bar baz]");
        let actions = AssignmentActions::new(&keys, op, &values).collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            Ok(Action(
                Key { name: "array", kind: Primitive::Str },
                Operator::Filter,
                "[foo bar baz]",
            ))
        );
    }
    #[test]
    fn repeated_key() {
        let (keys, op, vals) = split("x y z x = 1 2 3 4");
        let actions = AssignmentActions::new(&keys, op, &vals).collect::<Vec<_>>();
        assert_eq!(actions.len(), 4);
        assert_eq!(actions[3], Err(AssignmentError::RepeatedKey("x")))
    }

    #[test]
    fn no_key() {
        let (keys, op, vals) = split(" = 1");
        let actions = AssignmentActions::new(&keys, op, &vals).collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], Err(AssignmentError::NoKey("1")))
    }
}
