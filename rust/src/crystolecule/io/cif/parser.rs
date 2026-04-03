use std::collections::HashMap;
use thiserror::Error;

/// Error type for CIF parsing failures.
#[derive(Debug, Error)]
pub enum CifParseError {
    #[error("CIF parse error at line {line}: {message}")]
    Parse { line: usize, message: String },
}

/// A parsed CIF file containing one or more data blocks.
#[derive(Debug, Clone)]
pub struct CifDocument {
    pub data_blocks: Vec<CifDataBlock>,
}

/// A single data block (e.g., `data_diamond`).
#[derive(Debug, Clone)]
pub struct CifDataBlock {
    pub name: String,
    /// Single tag-value pairs (tags normalized to lowercase).
    pub tags: HashMap<String, String>,
    /// Tabular data sections.
    pub loops: Vec<CifLoop>,
}

/// A loop_ section with column headers and rows.
#[derive(Debug, Clone)]
pub struct CifLoop {
    /// Tag names (normalized to lowercase).
    pub columns: Vec<String>,
    /// Row values (each row has the same length as columns).
    pub rows: Vec<Vec<String>>,
}

/// Strip numeric uncertainty from a CIF value.
/// E.g., `5.4307(2)` → `5.4307`, `90.00(5)` → `90.00`.
/// Only strips trailing parenthesized integers from numeric-looking values.
fn strip_uncertainty(value: &str) -> String {
    if let Some(paren_start) = value.rfind('(') {
        if value.ends_with(')') {
            let before = &value[..paren_start];
            let inside = &value[paren_start + 1..value.len() - 1];
            // Only strip if the part before '(' looks numeric and inside is digits
            if inside.chars().all(|c| c.is_ascii_digit())
                && !before.is_empty()
                && before
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
            {
                return before.to_string();
            }
        }
    }
    value.to_string()
}

/// Parse a CIF file from a string.
pub fn parse_cif(input: &str) -> Result<CifDocument, CifParseError> {
    let mut tokenizer = Tokenizer::new(input);
    let tokens = tokenizer.tokenize()?;
    parse_tokens(&tokens)
}

// --- Tokenizer ---

#[derive(Debug, Clone, PartialEq)]
enum Token {
    DataBlock(String),   // data_blockname
    Loop,                // loop_
    Tag(String),         // _tag_name (lowercase)
    Value(String),       // a value (quoted or unquoted, uncertainties stripped)
}

struct TokenWithLine {
    token: Token,
    line: usize,
}

struct Tokenizer<'a> {
    lines: Vec<&'a str>,
    line_idx: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        let lines: Vec<&str> = input.lines().collect();
        Self {
            lines,
            line_idx: 0,
        }
    }

    fn tokenize(&mut self) -> Result<Vec<TokenWithLine>, CifParseError> {
        let mut tokens = Vec::new();

        while self.line_idx < self.lines.len() {
            let line = self.lines[self.line_idx];
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                self.line_idx += 1;
                continue;
            }

            // Semicolon-delimited text field (must start at column 0)
            if line.starts_with(';') {
                let start_line = self.line_idx;
                let mut text = String::new();
                self.line_idx += 1;
                // Collect the rest of the first semicolon line if there's content after ;
                let first_line_rest = line[1..].to_string();
                if !first_line_rest.trim().is_empty() {
                    text.push_str(&first_line_rest);
                }
                while self.line_idx < self.lines.len() {
                    let next_line = self.lines[self.line_idx];
                    if next_line.starts_with(';') {
                        break;
                    }
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(next_line);
                    self.line_idx += 1;
                }
                if self.line_idx >= self.lines.len() {
                    return Err(CifParseError::Parse {
                        line: start_line + 1,
                        message: "Unterminated semicolon text field".to_string(),
                    });
                }
                self.line_idx += 1; // skip closing ;
                tokens.push(TokenWithLine {
                    token: Token::Value(text.trim().to_string()),
                    line: start_line + 1,
                });
                continue;
            }

            // Parse the line into tokens
            let line_num = self.line_idx + 1;
            let mut chars = trimmed.char_indices().peekable();

            while chars.peek().is_some() {
                // Skip whitespace
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }

                if chars.peek().is_none() {
                    break;
                }

                let &(_, c) = chars.peek().unwrap();

                // Comment — skip rest of line
                if c == '#' {
                    break;
                }

                // Quoted string (single or double)
                if c == '\'' || c == '"' {
                    let quote = c;
                    chars.next(); // consume opening quote
                    let mut value = String::new();
                    let mut closed = false;
                    while let Some(&(_, ch)) = chars.peek() {
                        if ch == quote {
                            chars.next(); // consume closing quote
                            // In CIF, a closing quote must be followed by whitespace or end of line
                            match chars.peek() {
                                None | Some(&(_, ' ')) | Some(&(_, '\t')) => {
                                    closed = true;
                                    break;
                                }
                                _ => {
                                    // Quote is part of the value
                                    value.push(quote);
                                }
                            }
                        } else {
                            value.push(ch);
                            chars.next();
                        }
                    }
                    if !closed {
                        // End of line counts as closed
                        if chars.peek().is_none() {
                            // OK — quote was the last char or string ended at EOL
                        }
                    }
                    tokens.push(TokenWithLine {
                        token: Token::Value(value),
                        line: line_num,
                    });
                    continue;
                }

                // Collect an unquoted word
                let mut word = String::new();
                while let Some(&(_, ch)) = chars.peek() {
                    if ch.is_whitespace() {
                        break;
                    }
                    word.push(ch);
                    chars.next();
                }

                // Classify the word
                let word_lower = word.to_ascii_lowercase();
                if word_lower.starts_with("data_") {
                    tokens.push(TokenWithLine {
                        token: Token::DataBlock(word[5..].to_string()),
                        line: line_num,
                    });
                } else if word_lower == "loop_" {
                    tokens.push(TokenWithLine {
                        token: Token::Loop,
                        line: line_num,
                    });
                } else if word.starts_with('_') {
                    tokens.push(TokenWithLine {
                        token: Token::Tag(word_lower),
                        line: line_num,
                    });
                } else {
                    // It's a value — strip uncertainty
                    tokens.push(TokenWithLine {
                        token: Token::Value(strip_uncertainty(&word)),
                        line: line_num,
                    });
                }
            }

            self.line_idx += 1;
        }

        Ok(tokens)
    }
}

// --- Parser ---

fn parse_tokens(tokens: &[TokenWithLine]) -> Result<CifDocument, CifParseError> {
    let mut document = CifDocument {
        data_blocks: Vec::new(),
    };

    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i].token {
            Token::DataBlock(name) => {
                let mut block = CifDataBlock {
                    name: name.clone(),
                    tags: HashMap::new(),
                    loops: Vec::new(),
                };
                i += 1;

                // Parse block contents until next data_ or end
                while i < tokens.len() {
                    match &tokens[i].token {
                        Token::DataBlock(_) => break,
                        Token::Tag(tag) => {
                            // Tag-value pair
                            let tag_name = tag.clone();
                            i += 1;
                            if i >= tokens.len() {
                                return Err(CifParseError::Parse {
                                    line: tokens[i - 1].line,
                                    message: format!("Tag '{}' has no value", tag_name),
                                });
                            }
                            match &tokens[i].token {
                                Token::Value(val) => {
                                    block.tags.insert(tag_name, val.clone());
                                    i += 1;
                                }
                                _ => {
                                    return Err(CifParseError::Parse {
                                        line: tokens[i].line,
                                        message: format!(
                                            "Expected value after tag '{}', found {:?}",
                                            tag_name, tokens[i].token
                                        ),
                                    });
                                }
                            }
                        }
                        Token::Loop => {
                            i += 1;
                            // Collect column tags
                            let mut columns = Vec::new();
                            while i < tokens.len() {
                                if let Token::Tag(tag) = &tokens[i].token {
                                    columns.push(tag.clone());
                                    i += 1;
                                } else {
                                    break;
                                }
                            }
                            if columns.is_empty() {
                                return Err(CifParseError::Parse {
                                    line: tokens[i.saturating_sub(1)].line,
                                    message: "loop_ with no column tags".to_string(),
                                });
                            }
                            // Collect row values
                            let ncols = columns.len();
                            let mut rows: Vec<Vec<String>> = Vec::new();
                            let mut current_row: Vec<String> = Vec::new();
                            while i < tokens.len() {
                                match &tokens[i].token {
                                    Token::Value(val) => {
                                        current_row.push(val.clone());
                                        if current_row.len() == ncols {
                                            rows.push(current_row);
                                            current_row = Vec::new();
                                        }
                                        i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            // If there's a partial row, discard it (malformed data)
                            block.loops.push(CifLoop { columns, rows });
                        }
                        Token::Value(_) => {
                            // Stray value outside of tag or loop — skip
                            i += 1;
                        }
                    }
                }

                document.data_blocks.push(block);
            }
            _ => {
                // Tokens before the first data_ block — skip
                i += 1;
            }
        }
    }

    Ok(document)
}

impl CifDataBlock {
    /// Look up a single tag value by name (case-insensitive).
    pub fn get_tag(&self, tag: &str) -> Option<&str> {
        let key = tag.to_ascii_lowercase();
        self.tags.get(&key).map(|s| s.as_str())
    }

    /// Find a loop that contains the given tag.
    pub fn find_loop(&self, tag: &str) -> Option<&CifLoop> {
        let key = tag.to_ascii_lowercase();
        self.loops.iter().find(|l| l.columns.contains(&key))
    }
}

impl CifLoop {
    /// Get the column index for a given tag name.
    pub fn column_index(&self, tag: &str) -> Option<usize> {
        let key = tag.to_ascii_lowercase();
        self.columns.iter().position(|c| c == &key)
    }

    /// Get a column's values as an iterator of string slices.
    pub fn column_values(&self, tag: &str) -> Option<Vec<&str>> {
        let idx = self.column_index(tag)?;
        Some(self.rows.iter().map(|row| row[idx].as_str()).collect())
    }
}
