//! The message grammar shared by catalog validation and runtime interpolation.

enum Token<'a> {
    Text(&'a str),
    Placeholder(&'a str),
    Brace(char),
}

/// Parse named placeholders and doubled literal braces without interpreting inserted values.
fn tokens(text: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'{' && bytes[i] != b'}' {
            i += 1;
            continue;
        }
        if start < i {
            result.push(Token::Text(&text[start..i]));
        }
        let brace = bytes[i];
        if bytes.get(i + 1) == Some(&brace) {
            result.push(Token::Brace(brace as char));
            i += 2;
        } else if brace == b'{' {
            let end = text[i + 1..]
                .find('}')
                .map(|offset| i + 1 + offset)
                .ok_or_else(|| format!("unclosed placeholder at byte {i}"))?;
            let name = &text[i + 1..end];
            let mut letters = name.bytes();
            let valid_start = letters
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_');
            if !valid_start || !letters.all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                return Err(format!("invalid placeholder at byte {i}"));
            }
            result.push(Token::Placeholder(name));
            i = end + 1;
        } else {
            return Err(format!("unmatched closing brace at byte {i}"));
        }
        start = i;
    }
    if start < text.len() {
        result.push(Token::Text(&text[start..]));
    }
    Ok(result)
}

/// Return sorted names, preserving repeated placeholders; reject malformed message syntax.
#[allow(dead_code)] // Used by the build script and app tests, rather than normal app execution.
pub(crate) fn placeholder_names(text: &str) -> Result<Vec<String>, String> {
    let mut names = tokens(text)?
        .into_iter()
        .filter_map(|token| match token {
            Token::Placeholder(name) => Some(name.to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

/// Substitute values once and decode doubled braces in message text.
#[allow(dead_code)] // The build script uses validation, while the app also renders messages.
pub(crate) fn render(text: &str, values: &[(&str, &str)]) -> Result<String, String> {
    let mut output = String::new();
    for token in tokens(text)? {
        match token {
            Token::Text(text) => output.push_str(text),
            Token::Brace(brace) => output.push(brace),
            Token::Placeholder(name) => {
                if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
                    output.push_str(value);
                } else {
                    output.push('{');
                    output.push_str(name);
                    output.push('}');
                }
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_matches_shared_python_fixtures() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fixtures/locale-placeholders.json"))
                .unwrap();
        for case in cases {
            let text = case["text"].as_str().unwrap();
            let actual = placeholder_names(text);
            if case["names"].is_null() {
                assert!(actual.is_err(), "accepted malformed message {text:?}");
            } else {
                let expected: Vec<String> = serde_json::from_value(case["names"].clone()).unwrap();
                assert_eq!(actual.unwrap(), expected, "message {text:?}");
            }
        }
    }

    #[test]
    fn interpolation_preserves_values_and_decodes_escaped_braces() {
        assert_eq!(render("{{{count}}}", &[("count", "2")]).unwrap(), "{2}");
        assert_eq!(
            render("{title} {count}", &[("title", "A {count}"), ("count", "2")]).unwrap(),
            "A {count} 2"
        );
        assert_eq!(render("{missing}", &[]).unwrap(), "{missing}");
    }
}
