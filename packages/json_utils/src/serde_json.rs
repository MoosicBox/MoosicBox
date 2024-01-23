use serde_json::Value;

use crate::{ParseError, ToValueType};

pub trait ToNested<Type> {
    fn to_nested<'a>(&'a self, path: &[&str]) -> Result<&'a Type, ParseError>;
}

impl ToNested<Value> for &Value {
    fn to_nested<'a>(&'a self, path: &[&str]) -> Result<&'a Value, ParseError> {
        get_nested_value(self, path)
    }
}

pub fn get_nested_value<'a>(mut value: &'a Value, path: &[&str]) -> Result<&'a Value, ParseError> {
    for (i, x) in path.iter().enumerate() {
        if let Some(inner) = value.get(x) {
            value = inner;
            continue;
        }

        let message = if i > 0 {
            format!("Path '{}' missing value: '{}'", path[..i].join(" -> "), x)
        } else {
            format!("Missing value: '{}'", x)
        };

        return Err(ParseError::Parse(message));
    }

    Ok(value)
}

impl<'a> ToValueType<&'a str> for &'a Value {
    fn to_value_type(self) -> Result<&'a str, ParseError> {
        self.as_str()
            .ok_or_else(|| ParseError::ConvertType("&str".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<&'a str, ParseError> {
        Err(error)
    }
}

impl<'a> ToValueType<&'a Value> for &'a Value {
    fn to_value_type(self) -> Result<&'a Value, ParseError> {
        Ok(self)
    }

    fn missing_value(self, error: ParseError) -> Result<&'a Value, ParseError> {
        Err(error)
    }
}

impl<'a, T> ToValueType<Option<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Option<T>, ParseError> {
        self.to_value_type().map(|inner| Some(inner))
    }

    fn missing_value(self, _error: ParseError) -> Result<Option<T>, ParseError> {
        Ok(None)
    }
}

impl<'a, T> ToValueType<Vec<T>> for &'a Value
where
    &'a Value: ToValueType<T>,
{
    fn to_value_type(self) -> Result<Vec<T>, ParseError> {
        self.as_array()
            .ok_or_else(|| ParseError::ConvertType("Vec<T>".into()))?
            .iter()
            .map(|inner| inner.to_value_type())
            .collect::<Result<Vec<_>, _>>()
    }

    fn missing_value(self, error: ParseError) -> Result<Vec<T>, ParseError> {
        Err(error)
    }
}

impl ToValueType<String> for &Value {
    fn to_value_type(self) -> Result<String, ParseError> {
        Ok(self
            .as_str()
            .ok_or_else(|| ParseError::ConvertType("String".into()))?
            .to_string())
    }

    fn missing_value(self, error: ParseError) -> Result<String, ParseError> {
        Err(error)
    }
}

impl ToValueType<bool> for &Value {
    fn to_value_type(self) -> Result<bool, ParseError> {
        self.as_bool()
            .ok_or_else(|| ParseError::ConvertType("bool".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<bool, ParseError> {
        Err(error)
    }
}

impl ToValueType<f32> for &Value {
    fn to_value_type(self) -> Result<f32, ParseError> {
        Ok(self
            .as_f64()
            .ok_or_else(|| ParseError::ConvertType("f32".into()))? as f32)
    }

    fn missing_value(self, error: ParseError) -> Result<f32, ParseError> {
        Err(error)
    }
}

impl ToValueType<f64> for &Value {
    fn to_value_type(self) -> Result<f64, ParseError> {
        self.as_f64()
            .ok_or_else(|| ParseError::ConvertType("f64".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<f64, ParseError> {
        Err(error)
    }
}

impl ToValueType<u8> for &Value {
    fn to_value_type(self) -> Result<u8, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u8".into()))? as u8)
    }

    fn missing_value(self, error: ParseError) -> Result<u8, ParseError> {
        Err(error)
    }
}

impl ToValueType<u16> for &Value {
    fn to_value_type(self) -> Result<u16, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u16".into()))? as u16)
    }

    fn missing_value(self, error: ParseError) -> Result<u16, ParseError> {
        Err(error)
    }
}

impl ToValueType<u32> for &Value {
    fn to_value_type(self) -> Result<u32, ParseError> {
        Ok(self
            .as_u64()
            .ok_or_else(|| ParseError::ConvertType("u32".into()))? as u32)
    }

    fn missing_value(self, error: ParseError) -> Result<u32, ParseError> {
        Err(error)
    }
}

impl ToValueType<u64> for &Value {
    fn to_value_type(self) -> Result<u64, ParseError> {
        self.as_u64()
            .ok_or_else(|| ParseError::ConvertType("u64".into()))
    }

    fn missing_value(self, error: ParseError) -> Result<u64, ParseError> {
        Err(error)
    }
}

pub trait ToValue {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>;
}

impl ToValue for Value {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Self: ToValueType<T>,
    {
        self.to_nested_value(&[index])
    }
}

impl ToValue for &Value {
    fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        self.to_nested_value(&[index])
    }
}

pub trait ToNestedValue {
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>;
}

impl ToNestedValue for Value {
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Self: ToValueType<T>,
    {
        get_nested_value_type::<T>(self, path)
    }
}

impl ToNestedValue for &Value {
    fn to_nested_value<'a, T>(&'a self, path: &[&str]) -> Result<T, ParseError>
    where
        &'a Value: ToValueType<T>,
    {
        get_nested_value_type::<T>(self, path)
    }
}

pub fn get_nested_value_type<'a, T>(mut value: &'a Value, path: &[&str]) -> Result<T, ParseError>
where
    &'a Value: ToValueType<T>,
{
    for (i, x) in path.iter().enumerate() {
        if let Some(inner) = value.get(x) {
            value = inner;
            continue;
        }

        let message = if i > 0 {
            format!("Path '{}' missing value: '{}'", path[..i].join(" -> "), x)
        } else {
            format!("Missing value: '{}'", x)
        };

        return value.missing_value(ParseError::Parse(message));
    }

    if value.is_null() {
        return value.missing_value(ParseError::ConvertType("null".to_string()));
    }

    match value.to_value_type() {
        Ok(inner) => Ok(inner),
        Err(ParseError::ConvertType(r#type)) => Err(ParseError::ConvertType(format!(
            "Path '{}' failed to convert value to type: '{}'",
            path.join(" -> "),
            r#type,
        ))),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_nested_value_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64": 123,
            },
        });

        assert_eq!(
            json.to_nested_value::<u64>(&["outer", "inner_u64"])
                .unwrap(),
            123_u64
        );
    }

    #[test]
    fn test_to_value_option_null_string() {
        let json = &serde_json::json!({
            "str": serde_json::Value::Null,
        });

        assert_eq!(json.to_value::<Option<String>>("str").unwrap(), None);
    }

    #[test]
    fn test_to_value_option_string() {
        let json = &serde_json::json!({
            "str": "hey there",
            "u64": 123u64,
        });

        assert_eq!(
            json.to_value::<Option<String>>("str").unwrap(),
            Some("hey there".to_string())
        );

        assert_eq!(json.to_value::<Option<String>>("str2").unwrap(), None);

        assert_eq!(
            json.to_value::<Option<String>>("u64").err(),
            Some(ParseError::ConvertType(
                "Path 'u64' failed to convert value to type: 'String'".into()
            )),
        );

        let result: Option<String> = json.to_value("str").unwrap();
        assert_eq!(result, Some("hey there".to_string()));

        let result: Option<String> = json.to_value("str2").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_to_nested_value_option_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64": 123,
                "inner_str": "hey there",
            },
        });

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "inner_u64"])
                .unwrap(),
            Some(123_u64)
        );

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "bob"])
                .unwrap(),
            None
        );

        assert_eq!(
            json.to_nested_value::<Option<u64>>(&["outer", "inner_str"])
                .err(),
            Some(ParseError::ConvertType(
                "Path 'outer -> inner_str' failed to convert value to type: 'u64'".into()
            )),
        );
    }

    #[test]
    fn test_to_nested_value_vec_u64() {
        let json = &serde_json::json!({
            "outer": {
                "inner_u64_array": [123, 124, 125],
            },
        });

        assert_eq!(
            json.to_nested_value::<Vec<u64>>(&["outer", "inner_u64_array"])
                .unwrap(),
            vec![123_u64, 124_u64, 125_u64]
        );
    }

    #[test]
    fn test_to_value_nested_vec_u64() {
        let json = &serde_json::json!({
            "items": [
                {"item": 123},
                {"item": 124},
                {"item": 125},
            ],
        });

        let values = json.to_value::<Vec<&Value>>("items").unwrap();
        let numbers = values
            .into_iter()
            .map(|value| value.to_value::<u64>("item").unwrap())
            .collect::<Vec<_>>();

        assert_eq!(numbers, vec![123_u64, 124_u64, 125_u64]);
    }
}
