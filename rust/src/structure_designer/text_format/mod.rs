//! Text format serialization and parsing for the AI assistant integration.
//!
//! This module provides infrastructure for converting node networks to and from
//! a human-readable text format suitable for AI assistant consumption.
//!
//! # Components
//!
//! - [`TextValue`] - Enum representing typed values in the text format
//! - [`Parser`] - Parses text format input into statements
//! - Serialization functions in the [`serializer`] module
//!
//! # Text Format Overview
//!
//! The text format uses a simple assignment-based syntax:
//!
//! ```text
//! # Comments start with #
//! sphere1 = sphere { center: (0, 0, 0), radius: 5 }
//! box1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }
//! union1 = union { shapes: [sphere1, box1] }
//! output union1
//! ```
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use crate::structure_designer::text_format::{Parser, TextValue, Statement};
//!
//! // Parse text format input
//! let statements = Parser::parse("sphere1 = sphere { radius: 5 }")?;
//!
//! // Serialize a value to text
//! let value = TextValue::Int(42);
//! let text = value.to_text(); // "42"
//! ```

mod text_value;
mod serializer;
mod parser;

pub use text_value::TextValue;
pub use parser::{
    Parser,
    Lexer,
    ParseError,
    Statement,
    PropertyValue,
    Token,
    TokenInfo,
};
pub use serializer::TextFormatter;
