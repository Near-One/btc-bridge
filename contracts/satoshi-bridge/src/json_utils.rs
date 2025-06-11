use crate::*;

/// Recursively checks whether the structure of `input` matches the structure of `template`.
/// Values can differ, but keys and value types must conform to the `template`.
///
/// Rules:
/// 1. `input` may omit fields defined in the `template` (treated as optional).
/// 2. `input` must not contain extra fields not present in the `template`.
/// 3. If the template array has only one element, it's treated as a regular array:
///    all elements in `input` must match the type of the template element.
/// 4. If the template array has multiple elements, it's treated as an enum array:
///    all elements in `input` must match one of the enum variants.
/// 5. If a template value is `null`, then any corresponding input value is accepted (i.e., unconstrained).
pub fn is_structure_equal(template: &Value, input: &Value) -> bool {
    match (template, input) {
        (Value::Object(t_obj), Value::Object(i_obj)) => {
            for (key, t_val) in t_obj {
                match i_obj.get(key) {
                    Some(i_val) => {
                        if !is_structure_equal(t_val, i_val) {
                            return false;
                        }
                    }
                    None => {
                        // Input is allowed to omit fields defined in the template; these are treated as optional fields.
                        continue;
                    }
                }
            }
            // The input must not contain fields that are not defined in the template.
            for key in i_obj.keys() {
                if !t_obj.contains_key(key) {
                    return false;
                }
            }
            true
        }
        (Value::Array(t_arr), Value::Array(i_arr)) => {
            if t_arr.is_empty() {
                return i_arr.is_empty();
            }
            for i_item in i_arr {
                let mut matched = false;
                for t_item in t_arr {
                    if is_structure_equal(t_item, i_item) {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return false;
                }
            }
            true
        }
        (Value::String(_), Value::String(_)) => true,
        (Value::Number(_), Value::Number(_)) => true,
        (Value::Bool(_), Value::Bool(_)) => true,
        (Value::Null, _) => true, // When a key’s value is not restricted, set its value to null.
        _ => false,
    }
}
