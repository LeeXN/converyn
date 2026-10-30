//! Helm-style command-line values, without invoking a shell or template engine.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

fn split(input: &str, separator: char) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut part = String::new();
    let mut escaped = false;
    let mut depth = 0usize;
    for c in input.chars() {
        if escaped {
            part.push('\\');
            part.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == separator && depth == 0 {
            result.push(std::mem::take(&mut part));
        } else {
            if c == '{' {
                depth += 1;
            }
            if c == '}' {
                depth = depth.checked_sub(1).context("unmatched list brace")?;
            }
            part.push(c);
        }
    }
    ensure!(!escaped && depth == 0, "unfinished values escape or list");
    result.push(part);
    Ok(result)
}
fn unescape(input: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;
    for c in input.chars() {
        if c == '\\' && !escaped {
            escaped = true;
        } else {
            result.push(c);
            escaped = false;
        }
    }
    result
}
fn scalar(input: &str, string: bool) -> Value {
    let input = unescape(input);
    if !string {
        match input.to_ascii_lowercase().as_str() {
            "true" => return json!(true),
            "false" => return json!(false),
            "null" => return Value::Null,
            _ => {}
        }
        if (!input.starts_with('0') || input == "0")
            && let Ok(n) = input.parse::<i64>()
        {
            return json!(n);
        }
    }
    Value::String(input)
}
fn assign(target: &mut Value, path: &[String], value: Value) -> Result<()> {
    let Some((key, rest)) = path.split_first() else {
        *target = value;
        return Ok(());
    };
    if let Some(index) = key.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let index: usize = index.parse().context("invalid values array index")?;
        ensure!(index < 256, "values array index exceeds limit");
        if target.is_null() {
            *target = json!([]);
        }
        let values = target
            .as_array_mut()
            .context("values path expects an array")?;
        values.resize(values.len().max(index + 1), Value::Null);
        assign(&mut values[index], rest, value)
    } else {
        ensure!(!key.is_empty(), "empty values key");
        if target.is_null() {
            *target = json!({});
        }
        let values = target
            .as_object_mut()
            .context("values path expects an object")?;
        assign(
            values.entry(key.clone()).or_insert(Value::Null),
            rest,
            value,
        )
    }
}
pub fn set(target: &mut Value, input: &str, string: bool) -> Result<()> {
    for assignment in split(input, ',')? {
        let (key, raw) = assignment
            .split_once('=')
            .context("--set expects KEY=VALUE")?;
        let mut path = Vec::new();
        for part in split(key, '.')? {
            let mut key = String::new();
            let mut chars = part.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    key.push(chars.next().context("unfinished key escape")?);
                } else if c == '[' {
                    if !key.is_empty() {
                        path.push(std::mem::take(&mut key));
                    }
                    let mut index = String::from("[");
                    loop {
                        let c = chars.next().context("unfinished array index")?;
                        index.push(c);
                        if c == ']' {
                            break;
                        }
                    }
                    path.push(index);
                } else {
                    key.push(c);
                }
            }
            if !key.is_empty() {
                path.push(key);
            } else if !part.ends_with(']') {
                bail!("empty values key");
            }
        }
        ensure!(!path.is_empty() && path.len() <= 16, "invalid values path");
        let value = if raw.starts_with('{') && raw.ends_with('}') {
            Value::Array(
                split(&raw[1..raw.len() - 1], ',')?
                    .iter()
                    .map(|v| scalar(v, string))
                    .collect(),
            )
        } else {
            scalar(raw, string)
        };
        assign(target, &path, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_repeated_escaped_and_typed_values() {
        let mut values = json!({});
        set(
            &mut values,
            r"image.tag=v2,replicas=2,note=a\,b,labels.app\.name=demo,ports[0]=80",
            false,
        )
        .unwrap();
        set(
            &mut values,
            "replicas=3,enabled=true,names={one,two}",
            false,
        )
        .unwrap();
        set(&mut values, "code=123", true).unwrap();
        assert_eq!(
            values,
            json!({"image":{"tag":"v2"},"replicas":3,"note":"a,b","labels":{"app.name":"demo"},"ports":[80],"enabled":true,"names":["one","two"],"code":"123"})
        );
        assert!(set(&mut values, "ports[256]=bad", false).is_err());
        assert!(set(&mut values, "a..b=bad", false).is_err());
    }
}
