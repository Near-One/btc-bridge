use crate::*;

const INSERT_UTXO_TX_ID_TEMPLATE: &str = "{{UTXO_TX_ID}}";

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
pub fn check_template_and_update_msg(
    template: &Value,
    input: &Value,
    utxo_tx_id: &str,
) -> Option<Value> {
    let mut res = input.clone();
    match (template, input) {
        (Value::Object(t_obj), Value::Object(i_obj)) => {
            for (key, t_val) in t_obj {
                match i_obj.get(key) {
                    Some(i_val) => match check_template_and_update_msg(t_val, i_val, utxo_tx_id) {
                        Some(val) => {
                            res.as_object_mut().unwrap().insert(key.clone(), val);
                        }
                        None => return None,
                    },
                    None => {
                        // Input is allowed to omit fields defined in the template; these are treated as optional fields.
                        continue;
                    }
                }
            }
            // The input must not contain fields that are not defined in the template.
            for key in i_obj.keys() {
                if !t_obj.contains_key(key) {
                    return None;
                }
            }
            Some(res)
        }
        (Value::Array(t_arr), Value::Array(i_arr)) => {
            if t_arr.is_empty() {
                if i_arr.is_empty() {
                    return Some(res);
                }
                return None;
            }
            res = Value::Array(vec![]);

            for i_item in i_arr {
                let mut matched = false;
                for t_item in t_arr {
                    if let Some(sub_res) = check_template_and_update_msg(t_item, i_item, utxo_tx_id)
                    {
                        res.as_array_mut().unwrap().push(sub_res);

                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return None;
                }
            }
            Some(res)
        }
        (Value::String(temp_str), Value::String(real_str)) => {
            if temp_str == INSERT_UTXO_TX_ID_TEMPLATE && real_str == INSERT_UTXO_TX_ID_TEMPLATE {
                res = Value::String(utxo_tx_id.to_string());
            }
            Some(res)
        }
        (Value::Number(_), Value::Number(_)) => Some(res),
        (Value::Bool(_), Value::Bool(_)) => Some(res),
        (Value::Null, _) => Some(res), // When a key’s value is not restricted, set its value to null.
        _ => None,
    }
}
