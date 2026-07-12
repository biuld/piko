use super::types::PromptTemplate;

pub fn expand_prompt_template(text: &str, templates: &[PromptTemplate]) -> String {
    if !text.starts_with('/') {
        return text.to_string();
    }
    let mut parts = text[1..].splitn(2, char::is_whitespace);
    let Some(name) = parts.next() else {
        return text.to_string();
    };
    let args_string = parts.next().unwrap_or("").trim();
    let Some(template) = templates.iter().find(|template| template.name == name) else {
        return text.to_string();
    };
    substitute_args(&template.content, &parse_command_args(args_string))
}
pub(crate) fn parse_frontmatter_result(
    content: &str,
) -> Result<(std::collections::HashMap<String, String>, String), String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return Ok((std::collections::HashMap::new(), normalized));
    }
    let Some(end) = normalized[3..].find("\n---") else {
        return Ok((std::collections::HashMap::new(), normalized));
    };
    let yaml_start = 4.min(normalized.len());
    let yaml_end = 3 + end;
    let yaml = &normalized[yaml_start..yaml_end];
    let body = normalized[yaml_end + 4..].trim().to_string();
    let value =
        serde_yaml::from_str::<serde_yaml::Value>(yaml).map_err(|error| error.to_string())?;
    let Some(mapping) = value.as_mapping() else {
        return Ok((std::collections::HashMap::new(), body));
    };
    let mut map = std::collections::HashMap::new();
    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        if let Some(value) = yaml_value_to_string(value) {
            map.insert(key.to_string(), value);
        }
    }
    Ok((map, body))
}

fn yaml_value_to_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::Null => Some(String::new()),
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        serde_yaml::Value::String(value) => Some(value.clone()),
        serde_yaml::Value::Sequence(values) => Some(
            values
                .iter()
                .filter_map(yaml_value_to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Tagged(_) => None,
    }
}

fn parse_command_args(args: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in args.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn substitute_args(content: &str, args: &[String]) -> String {
    let mut result = substitute_numeric_args(content, args);
    result = substitute_arg_slices(&result, args);
    let all = args.join(" ");
    result.replace("$ARGUMENTS", &all).replace("$@", &all)
}

fn substitute_numeric_args(content: &str, args: &[String]) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' || !chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            result.push(ch);
            continue;
        }

        let mut digits = String::new();
        while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            if let Some(digit) = chars.next() {
                digits.push(digit);
            }
        }

        let index = digits.parse::<usize>().unwrap_or(0).saturating_sub(1);
        if let Some(arg) = args.get(index) {
            result.push_str(arg);
        }
    }

    result
}

fn substitute_arg_slices(content: &str, args: &[String]) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("${@:") {
        result.push_str(&rest[..start]);
        let after_marker = &rest[start + 4..];
        let Some(end) = after_marker.find('}') else {
            result.push_str(&rest[start..]);
            return result;
        };

        let expression = &after_marker[..end];
        let replacement = parse_arg_slice_expression(expression)
            .map(|(start, length)| {
                let zero_based = start.saturating_sub(1);
                match length {
                    Some(length) => args
                        .iter()
                        .skip(zero_based)
                        .take(length)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                    None => args
                        .iter()
                        .skip(zero_based)
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                }
            })
            .unwrap_or_else(|| rest[start..start + 4 + end + 1].to_string());

        result.push_str(&replacement);
        rest = &after_marker[end + 1..];
    }

    result.push_str(rest);
    result
}

fn parse_arg_slice_expression(expression: &str) -> Option<(usize, Option<usize>)> {
    let (start, length) = expression
        .split_once(':')
        .map_or((expression, None), |(start, length)| (start, Some(length)));
    if start.is_empty() || !start.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let start = start.parse::<usize>().ok()?;
    let length = match length {
        Some(length) if length.chars().all(|ch| ch.is_ascii_digit()) => {
            Some(length.parse::<usize>().ok()?)
        }
        Some(_) => return None,
        None => None,
    };
    Some((start, length))
}
