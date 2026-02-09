use super::text_value::TextValue;

/// Serialize a TextValue to its text format string representation.
impl TextValue {
    /// Serialize to text format string
    pub fn to_text(&self) -> String {
        match self {
            TextValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            TextValue::Int(i) => i.to_string(),
            TextValue::Float(f) => format_float(*f),
            TextValue::String(s) => format_string(s),
            TextValue::IVec2(v) => format!("({}, {})", v.x, v.y),
            TextValue::IVec3(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            TextValue::Vec2(v) => format!("({}, {})", format_float(v.x), format_float(v.y)),
            TextValue::Vec3(v) => format!(
                "({}, {}, {})",
                format_float(v.x),
                format_float(v.y),
                format_float(v.z)
            ),
            TextValue::DataType(dt) => dt.to_string(),
            TextValue::Array(arr) => format_array(arr),
            TextValue::Object(obj) => format_object(obj),
        }
    }
}

/// Format a float ensuring it has a decimal point (to distinguish from int).
/// This is important for type inference during parsing.
pub fn format_float(f: f64) -> String {
    // Handle special cases
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_string();
    }

    let s = f.to_string();
    // If already has decimal point or exponent, return as-is
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        // Add .0 to make it clear this is a float
        format!("{}.0", s)
    }
}

/// Format a string with proper escaping and multi-line handling.
pub fn format_string(s: &str) -> String {
    if s.contains('\n') {
        // Use triple-quoted string for multi-line content
        // Triple quotes don't need internal escaping except for triple quotes themselves
        let escaped = s.replace("\"\"\"", "\\\"\\\"\\\"");
        format!("\"\"\"{}\"\"\"", escaped)
    } else {
        // Use regular quoted string with escaping
        format!("\"{}\"", escape_string(s))
    }
}

/// Escape special characters in a single-line string.
fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            // All other characters pass through
            _ => result.push(ch),
        }
    }
    result
}

/// Format an array of TextValues as `[val1, val2, ...]`
fn format_array(arr: &[TextValue]) -> String {
    if arr.is_empty() {
        return "[]".to_string();
    }

    let elements: Vec<String> = arr.iter().map(|v| v.to_text()).collect();
    format!("[{}]", elements.join(", "))
}

/// Format an object as `{ key1: val1, key2: val2 }`
fn format_object(obj: &[(String, TextValue)]) -> String {
    if obj.is_empty() {
        return "{}".to_string();
    }

    let entries: Vec<String> = obj
        .iter()
        .map(|(key, val)| format!("{}: {}", key, val.to_text()))
        .collect();
    format!("{{ {} }}", entries.join(", "))
}

/// A helper struct for building text format output with proper indentation.
/// Useful for formatting entire node networks with readability.
pub struct TextFormatter {
    output: String,
    indent_level: usize,
    indent_str: String,
}

impl TextFormatter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            indent_str: "  ".to_string(), // 2 spaces
        }
    }

    /// Create a formatter with custom indentation string
    pub fn with_indent(indent: &str) -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            indent_str: indent.to_string(),
        }
    }

    /// Increase indentation level
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    /// Decrease indentation level
    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Write a line with current indentation
    pub fn writeln(&mut self, text: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.indent_str);
        }
        self.output.push_str(text);
        self.output.push('\n');
    }

    /// Write text without newline, with current indentation
    pub fn write(&mut self, text: &str) {
        for _ in 0..self.indent_level {
            self.output.push_str(&self.indent_str);
        }
        self.output.push_str(text);
    }

    /// Write text without any indentation
    pub fn write_raw(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// Add a blank line
    pub fn blank_line(&mut self) {
        self.output.push('\n');
    }

    /// Get the final output
    pub fn finish(self) -> String {
        self.output
    }

    /// Get the current output (for inspection without consuming)
    pub fn current(&self) -> &str {
        &self.output
    }
}

impl Default for TextFormatter {
    fn default() -> Self {
        Self::new()
    }
}
