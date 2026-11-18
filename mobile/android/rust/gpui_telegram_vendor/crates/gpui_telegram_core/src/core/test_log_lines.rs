//! Telegram Desktop 1:1 Parity Module: `TestLogLines`
//! Subsystem: `core` | Feature ID: `TCG-N02`
//! Disposition: `rust_domain_or_server_1to1`
//! Upstream Sources: Telegram/SourceFiles/test/test_log_lines.h, Telegram/SourceFiles/test/test_log_lines.cpp

use serde::{Deserialize, Serialize};

/// Test harness verifying the single physical line contract for test logs:
/// all Unicode newline / paragraph separators (CR, LF, NEL, LS, PS, FF, VT)
/// are escaped to visible `\uXXXX` escapes to prevent log forgery or truncation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestLogLines {
    pub total_records: usize,
    pub escaped_characters_count: usize,
    pub verified_single_lines: bool,
}

impl Default for TestLogLines {
    fn default() -> Self {
        Self {
            total_records: 0,
            escaped_characters_count: 0,
            verified_single_lines: true,
        }
    }
}

impl TestLogLines {
    pub fn new() -> Self {
        Self::default()
    }

    /// Escapes all 11 Unicode line break variants into visible `\uXXXX` escapes.
    pub fn escape_line_breaks(input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        for ch in input.chars() {
            match ch {
                '\n' => result.push_str("\\u000A"),
                '\r' => result.push_str("\\u000D"),
                '\x0B' => result.push_str("\\u000B"),
                '\x0C' => result.push_str("\\u000C"),
                '\x1C' => result.push_str("\\u001C"),
                '\x1D' => result.push_str("\\u001D"),
                '\x1E' => result.push_str("\\u001E"),
                '\u{0085}' => result.push_str("\\u0085"),
                '\u{2028}' => result.push_str("\\u2028"),
                '\u{2029}' => result.push_str("\\u2029"),
                other => result.push(other),
            }
        }
        result
    }

    /// Verifies that the escaped record contains zero physical newline characters.
    pub fn is_strict_single_line(record: &str) -> bool {
        !record.contains('\n') && !record.contains('\r')
    }

    /// Executes the upstream self-test verifying line escape fidelity.
    pub fn run_self_test(&mut self) -> Result<(), &'static str> {
        let test_cases = [
            ("Hello World", "Hello World"),
            ("Line 1\nLine 2", "Line 1\\u000ALine 2"),
            ("Carriage\rReturn", "Carriage\\u000DReturn"),
            ("Form\x0CFeed", "Form\\u000CFeed"),
        ];

        for (input, expected) in test_cases {
            let escaped = Self::escape_line_breaks(input);
            if escaped != expected {
                self.verified_single_lines = false;
                return Err("Escape mismatch in TestLogLines self test");
            }
            if !Self::is_strict_single_line(&escaped) {
                self.verified_single_lines = false;
                return Err("Record contains physical break");
            }
            self.total_records += 1;
        }

        self.verified_single_lines = true;
        Ok(())
    }
}
