use std::collections::HashMap;

pub fn parse_options_header(value: &str) -> Result<(String, HashMap<String, String>), String> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character == ';' && !quoted {
            segments.push(&value[start..index]);
            start = index + 1;
        }
    }
    if quoted || escaped {
        return Err("Malformed quoted parameter".to_string());
    }
    segments.push(&value[start..]);

    let name = segments.remove(0).trim();
    if name.is_empty() {
        return Err("Missing header name".to_string());
    }

    let mut parameters = HashMap::new();
    for segment in segments {
        let (key, raw_value) = segment.split_once('=').ok_or("Missing parameter value")?;
        let key = key.trim();
        if key.is_empty() {
            return Err("Missing parameter key".to_string());
        }

        let raw_value = raw_value.trim();
        let parameter = if raw_value.starts_with('"') {
            if raw_value.len() < 2 || !raw_value.ends_with('"') {
                return Err("Malformed quoted parameter".to_string());
            }
            let mut parameter = String::new();
            let mut characters = raw_value[1..raw_value.len() - 1].chars();
            while let Some(character) = characters.next() {
                if character == '\\' {
                    parameter.push(characters.next().ok_or("Malformed quoted parameter")?);
                } else {
                    parameter.push(character);
                }
            }
            parameter
        } else {
            raw_value.to_string()
        };
        parameters.insert(key.to_ascii_lowercase(), parameter);
    }

    Ok((name.to_string(), parameters))
}
