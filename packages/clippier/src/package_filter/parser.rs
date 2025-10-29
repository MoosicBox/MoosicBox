//! Filter string parsing.

use super::types::{FilterError, FilterOperator, PackageFilter};

/// Parse a filter string into a structured filter.
///
/// # Format
///
/// `property[.nested[.deeper]]<operator>value`
///
/// # Examples
///
/// * `"publish=false"` - Check if publish is false
/// * `"version^=0.1"` - Check if version starts with "0.1"
/// * `"categories@=audio"` - Check if categories array contains "audio"
/// * `"metadata.workspaces.independent=true"` - Nested property check
/// * `"readme?"` - Check if readme property exists
///
/// # Errors
///
/// * Returns error if filter syntax is invalid
/// * Returns error if property name is empty
/// * Returns error if value is provided for value-optional operators
pub fn parse_filter(filter: &str) -> Result<PackageFilter, FilterError> {
    // Try operators in order of specificity (longest first to avoid partial matches)
    // Note: !? must come before !, and ? must come after != to avoid conflicts
    let operators = [
        ("!@=", FilterOperator::ArrayNotContains),
        ("!?", FilterOperator::NotExists),
        ("@~=", FilterOperator::ArrayContainsRegex),
        ("@^=", FilterOperator::ArrayContainsStartsWith),
        ("@*=", FilterOperator::ArrayContainsSubstring),
        ("@#>", FilterOperator::ArrayLengthGreater),
        ("@#<", FilterOperator::ArrayLengthLess),
        ("@#=", FilterOperator::ArrayLengthEquals),
        ("@=", FilterOperator::ArrayContains),
        ("@!", FilterOperator::ArrayEmpty),
        ("~=", FilterOperator::RegexMatch),
        ("^=", FilterOperator::StartsWith),
        ("$=", FilterOperator::EndsWith),
        ("*=", FilterOperator::Contains),
        ("!=", FilterOperator::NotEquals),
        ("=", FilterOperator::Equals),
        ("?", FilterOperator::Exists),
    ];

    for (op_str, operator) in &operators {
        if let Some((property, value)) = filter.split_once(op_str) {
            let property = property.trim();
            let value = value.trim();

            // Validate property name
            if property.is_empty() {
                return Err(FilterError::InvalidSyntax(
                    "Property name cannot be empty".to_string(),
                ));
            }

            // Split property path by dots
            let property_path: Vec<String> =
                property.split('.').map(|s| s.trim().to_string()).collect();

            // Validate each part of the path
            for part in &property_path {
                if part.is_empty() {
                    return Err(FilterError::InvalidSyntax(
                        "Property path cannot contain empty segments".to_string(),
                    ));
                }
            }

            // For operators that don't need values, value should be empty
            if operator.is_value_optional() && !value.is_empty() {
                return Err(FilterError::InvalidSyntax(format!(
                    "{op_str} operator should not have a value"
                )));
            }

            return Ok(PackageFilter {
                property_path,
                operator: *operator,
                value: value.to_string(),
            });
        }
    }

    Err(FilterError::InvalidSyntax(format!(
        "No valid operator found in filter: '{filter}'"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_equality() {
        let filter = parse_filter("publish=false").unwrap();
        assert_eq!(filter.property_path, vec!["publish"]);
        assert_eq!(filter.operator, FilterOperator::Equals);
        assert_eq!(filter.value, "false");
    }

    #[test]
    fn test_parse_array_contains() {
        let filter = parse_filter("categories@=audio").unwrap();
        assert_eq!(filter.property_path, vec!["categories"]);
        assert_eq!(filter.operator, FilterOperator::ArrayContains);
        assert_eq!(filter.value, "audio");
    }

    #[test]
    fn test_parse_array_empty() {
        let filter = parse_filter("keywords@!").unwrap();
        assert_eq!(filter.property_path, vec!["keywords"]);
        assert_eq!(filter.operator, FilterOperator::ArrayEmpty);
        assert_eq!(filter.value, "");
    }

    #[test]
    fn test_parse_nested_property() {
        let filter = parse_filter("metadata.workspaces.independent=true").unwrap();
        assert_eq!(
            filter.property_path,
            vec!["metadata", "workspaces", "independent"]
        );
        assert_eq!(filter.operator, FilterOperator::Equals);
        assert_eq!(filter.value, "true");
    }

    #[test]
    fn test_parse_exists() {
        let filter = parse_filter("readme?").unwrap();
        assert_eq!(filter.property_path, vec!["readme"]);
        assert_eq!(filter.operator, FilterOperator::Exists);
        assert_eq!(filter.value, "");
    }

    #[test]
    fn test_parse_not_exists() {
        let filter = parse_filter("homepage!?").unwrap();
        assert_eq!(filter.property_path, vec!["homepage"]);
        assert_eq!(filter.operator, FilterOperator::NotExists);
        assert_eq!(filter.value, "");
    }

    #[test]
    fn test_parse_invalid_empty_property() {
        let result = parse_filter("=value");
        assert!(matches!(result, Err(FilterError::InvalidSyntax(_))));
    }

    #[test]
    fn test_parse_invalid_value_with_exists() {
        let result = parse_filter("readme?value");
        assert!(matches!(result, Err(FilterError::InvalidSyntax(_))));
    }

    #[test]
    fn test_parse_invalid_empty_path_segment() {
        let result = parse_filter("metadata..independent=true");
        assert!(matches!(result, Err(FilterError::InvalidSyntax(_))));
    }
}
