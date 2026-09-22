use std::collections::BTreeMap;

use anyhow::{Result, bail, ensure};

pub(crate) fn values(text: &str) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }
        let equal = line.find('=');
        let multiline = line.find("<<");
        if let Some(index) = equal.filter(|i| multiline.is_none_or(|m| *i < m)) {
            let (key, value) = line.split_at(index);
            ensure!(!key.is_empty(), "empty GitHub output key");
            values.insert(key.to_string(), value[1..].to_string());
        } else if let Some(index) = multiline {
            let key = &line[..index];
            let delimiter = &line[index + 2..];
            ensure!(
                !key.is_empty() && !delimiter.is_empty(),
                "invalid multiline output header"
            );
            let mut content = Vec::new();
            loop {
                let next = lines
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("unterminated multiline output: {key}"))?;
                if next == delimiter {
                    break;
                }
                content.push(next);
            }
            values.insert(key.to_string(), content.join("\n"));
        } else {
            bail!("invalid GitHub output line: {line:?}");
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_and_last_value() {
        let values = values("a=old\r\na=new\r\nb<<END\r\nfirst\r\nsecond\r\nEND\r\n").unwrap();
        assert_eq!(values["a"], "new");
        assert_eq!(values["b"], "first\nsecond");
    }

    #[test]
    fn rejects_malformed_output() {
        for text in ["bad", "=empty", "a<<END\nmissing", "a<<\n"] {
            assert!(values(text).is_err(), "{text}");
        }
    }
}
