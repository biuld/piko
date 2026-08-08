use super::*;

pub struct CommandSegment {
    pub binary: String,
    pub args: Vec<String>,
    pub redirects: Vec<(String, String)>, // (operator, target)
}

impl CommandSegment {
    pub fn canonicalize(&self) -> String {
        let mut result = self.binary.clone();
        let mut args = self.args.clone();

        // If the first argument is a subcommand (doesn't start with -), keep it attached
        if let Some(first) = args.first()
            && !first.starts_with('-')
        {
            result.push(' ');
            result.push_str(first);
            args.remove(0);
        }

        // Separate and sort options/flags
        let mut options = Vec::new();
        let mut positional = Vec::new();

        let mut iter = args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            if arg.starts_with('-') {
                // If it's a flag with value (e.g. -p value or --port value), pair them
                if let Some(next) = iter.peek()
                    && !next.starts_with('-')
                {
                    let val = iter.next().unwrap();
                    options.push(format!("{} {}", arg, val));
                    continue;
                }
                options.push(arg);
            } else {
                positional.push(arg);
            }
        }

        options.sort();

        for opt in options {
            result.push(' ');
            result.push_str(&opt);
        }

        for pos in positional {
            result.push(' ');
            result.push_str(&pos);
        }

        // Canonicalize and sort redirects
        let mut redirects = self.redirects.clone();
        redirects.sort();
        for (op, target) in redirects {
            result.push(' ');
            result.push_str(&op);
            result.push(' ');
            result.push_str(&target);
        }

        result
    }
}

pub fn parse_shell_command(command: &str) -> Result<Vec<CommandSegment>, PolicyError> {
    let raw_tokens = tokenize(command).map_err(PolicyError::Shell)?;
    let mut tokens = Vec::new();
    for token in raw_tokens {
        let words = shell_words::split(&token).map_err(|e| PolicyError::Shell(e.to_string()))?;
        if let Some(word) = words.first() {
            tokens.push(word.clone());
        }
    }

    let mut segments = Vec::new();
    let mut current_binary: Option<String> = None;
    let mut current_args = Vec::new();
    let mut current_redirects = Vec::new();

    let mut iter = tokens.into_iter().peekable();
    while let Some(token) = iter.next() {
        match token.as_str() {
            "|" | "&&" | "||" | ";" | "&" => {
                if let Some(bin) = current_binary.take() {
                    segments.push(CommandSegment {
                        binary: bin,
                        args: std::mem::take(&mut current_args),
                        redirects: std::mem::take(&mut current_redirects),
                    });
                }
            }
            ">" | ">>" | "<" | "2>" | "2>>" => {
                let target = iter.next().ok_or_else(|| {
                    PolicyError::Shell(format!("missing target for redirect '{}'", token))
                })?;
                current_redirects.push((token, target));
            }
            _ => {
                if current_binary.is_none() {
                    current_binary = Some(token);
                } else {
                    current_args.push(token);
                }
            }
        }
    }

    if let Some(bin) = current_binary {
        segments.push(CommandSegment {
            binary: bin,
            args: current_args,
            redirects: current_redirects,
        });
    }

    Ok(segments)
}

pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            current.push(c);
            escaped = false;
            i += 1;
            continue;
        }

        if c == '\\' && !in_single_quote {
            escaped = true;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if c == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(c);
            i += 1;
            continue;
        }

        if in_single_quote || in_double_quote {
            current.push(c);
            i += 1;
            continue;
        }

        if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            i += 1;
            continue;
        }

        let mut matched_operator = None;
        for op in &["&&", "||", ">>", "2>>", "2>"] {
            let len = op.len();
            if i + len <= chars.len() {
                let segment: String = chars[i..i + len].iter().collect();
                if segment == *op {
                    matched_operator = Some((op, len));
                    break;
                }
            }
        }

        if matched_operator.is_none() {
            for op in &["|", ";", "&", ">", "<"] {
                if c == op.chars().next().unwrap() {
                    matched_operator = Some((op, 1));
                    break;
                }
            }
        }

        if let Some((op, len)) = matched_operator {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(op.to_string());
            i += len;
        } else {
            current.push(c);
            i += 1;
        }
    }

    if in_single_quote || in_double_quote {
        return Err("unclosed quote".to_string());
    }
    if escaped {
        return Err("trailing backslash".to_string());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}
