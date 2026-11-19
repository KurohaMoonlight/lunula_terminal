//! Ultra-lightweight, zero-dependency syntax highlighter for `luna`.
//! Employs Kanagawa Wave colors with fast per-line lexical tokenization.

use crate::theme::Theme;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Plain,
    Lua,
    Rust,
    Shell,
    Config, // JSON, TOML, YAML
    Python,
    Markdown,
}

impl Language {
    pub fn from_path(path_opt: Option<&Path>) -> Self {
        let Some(p) = path_opt else { return Self::Plain };
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let fname = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if fname.ends_with(".omp.json") || fname == "cargo.lock" {
            return Self::Config;
        }

        match ext.as_str() {
            "lua" => Self::Lua,
            "rs" => Self::Rust,
            "ps1" | "psm1" | "sh" | "bash" | "zsh" | "cmd" | "bat" => Self::Shell,
            "json" | "toml" | "yaml" | "yml" | "ini" | "conf" => Self::Config,
            "py" => Self::Python,
            "md" | "markdown" => Self::Markdown,
            _ => Self::Plain,
        }
    }

    pub fn display_tag(self) -> &'static str {
        match self {
            Self::Lua => " [Lua]",
            Self::Rust => " [Rust]",
            Self::Shell => " [Shell]",
            Self::Config => " [Config]",
            Self::Python => " [Python]",
            Self::Markdown => " [Markdown]",
            Self::Plain => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Normal,
    Keyword,
    Type,
    String,
    Number,
    Comment,
    Punctuation,
    Header,
}

impl TokenType {
    pub fn ansi_color(self, theme: &Theme) -> String {
        match self {
            TokenType::Normal => theme.fg.to_fg_ansi(),
            TokenType::Keyword => format!("{}\x1b[1m", theme.syntax_keyword.to_fg_ansi()),
            TokenType::Type => theme.syntax_type.to_fg_ansi(),
            TokenType::String => theme.syntax_string.to_fg_ansi(),
            TokenType::Number => theme.syntax_number.to_fg_ansi(),
            TokenType::Comment => format!("{}\x1b[3m", theme.syntax_comment.to_fg_ansi()),
            TokenType::Punctuation => theme.syntax_punctuation.to_fg_ansi(),
            TokenType::Header => format!("{}\x1b[1m", theme.syntax_header.to_fg_ansi()),
        }
    }
}

fn is_lua_keyword(w: &str) -> bool {
    matches!(
        w,
        "and"
            | "break"
            | "do"
            | "else"
            | "elseif"
            | "end"
            | "false"
            | "for"
            | "function"
            | "if"
            | "in"
            | "local"
            | "nil"
            | "not"
            | "or"
            | "repeat"
            | "return"
            | "then"
            | "true"
            | "until"
            | "while"
    )
}

fn is_rust_keyword(w: &str) -> bool {
    matches!(
        w,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

fn is_rust_type(w: &str) -> bool {
    matches!(
        w,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "Box"
            | "Rc"
            | "Arc"
            | "Cell"
            | "RefCell"
    )
}

fn is_shell_keyword(w: &str) -> bool {
    let lower = w.to_lowercase();
    matches!(
        lower.as_str(),
        "if" | "then"
            | "else"
            | "elseif"
            | "elif"
            | "fi"
            | "for"
            | "in"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "while"
            | "until"
            | "function"
            | "return"
            | "exit"
            | "echo"
            | "param"
            | "begin"
            | "process"
            | "end"
            | "switch"
            | "try"
            | "catch"
            | "finally"
            | "throw"
            | "$true"
            | "$false"
            | "$null"
    )
}

fn is_python_keyword(w: &str) -> bool {
    matches!(
        w,
        "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "from"
            | "as"
            | "try"
            | "except"
            | "finally"
            | "with"
            | "pass"
            | "break"
            | "continue"
            | "lambda"
            | "yield"
            | "True"
            | "False"
            | "None"
            | "not"
            | "and"
            | "or"
            | "is"
            | "in"
    )
}

fn is_config_keyword(w: &str) -> bool {
    matches!(w, "true" | "false" | "null")
}

/// Tokenizes a single text line into contiguous (TokenType, String) slices.
pub fn highlight_line(line: &str, lang: Language) -> Vec<(TokenType, String)> {
    if lang == Language::Plain || line.is_empty() {
        return vec![(TokenType::Normal, line.to_string())];
    }

    // Markdown Header detection
    if lang == Language::Markdown {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            return vec![(TokenType::Header, line.to_string())];
        }
    }

    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // 1. Comments
        let is_comment = match lang {
            Language::Lua => ch == '-' && i + 1 < len && chars[i + 1] == '-',
            Language::Rust => ch == '/' && i + 1 < len && chars[i + 1] == '/',
            Language::Shell | Language::Python | Language::Config => ch == '#',
            Language::Markdown => false,
            Language::Plain => false,
        };

        if is_comment {
            let comment_text: String = chars[i..].iter().collect();
            tokens.push((TokenType::Comment, comment_text));
            break;
        }

        // 2. Strings ('...', "...", `...`)
        if ch == '"' || ch == '\'' || (ch == '`' && lang != Language::Plain) {
            let quote = ch;
            let mut j = i + 1;
            let mut escaped = false;
            while j < len {
                let c2 = chars[j];
                if escaped {
                    escaped = false;
                } else if c2 == '\\' {
                    escaped = true;
                } else if c2 == quote {
                    j += 1;
                    break;
                }
                j += 1;
            }
            let s_text: String = chars[i..j].iter().collect();
            tokens.push((TokenType::String, s_text));
            i = j;
            continue;
        }

        // 3. Numbers (Hex: 0x..., Digits)
        if ch.is_ascii_digit() {
            let mut j = i;
            let is_hex = ch == '0'
                && i + 1 < len
                && (chars[i + 1] == 'x' || chars[i + 1] == 'X');

            if is_hex {
                j += 2;
                while j < len && chars[j].is_ascii_hexdigit() {
                    j += 1;
                }
            } else {
                while j < len && (chars[j].is_ascii_digit() || chars[j] == '.' || chars[j] == '_') {
                    j += 1;
                }
            }
            let num_text: String = chars[i..j].iter().collect();
            tokens.push((TokenType::Number, num_text));
            i = j;
            continue;
        }

        // 4. Identifiers / Keywords / Shell Variables
        if ch.is_alphabetic() || ch == '_' || (ch == '$' && lang == Language::Shell) {
            let mut j = i;
            while j < len && (chars[j].is_alphanumeric() || chars[j] == '_' || (j == i && chars[j] == '$')) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();

            let token_type = match lang {
                Language::Lua => {
                    if is_lua_keyword(&word) {
                        TokenType::Keyword
                    } else {
                        TokenType::Normal
                    }
                }
                Language::Rust => {
                    if is_rust_keyword(&word) {
                        TokenType::Keyword
                    } else if is_rust_type(&word) || word.chars().next().map_or(false, |c| c.is_uppercase()) {
                        TokenType::Type
                    } else {
                        TokenType::Normal
                    }
                }
                Language::Shell => {
                    if word.starts_with('$') {
                        TokenType::Type
                    } else if is_shell_keyword(&word) {
                        TokenType::Keyword
                    } else {
                        TokenType::Normal
                    }
                }
                Language::Python => {
                    if is_python_keyword(&word) {
                        TokenType::Keyword
                    } else {
                        TokenType::Normal
                    }
                }
                Language::Config => {
                    if is_config_keyword(&word) {
                        TokenType::Keyword
                    } else {
                        TokenType::Normal
                    }
                }
                _ => TokenType::Normal,
            };

            tokens.push((token_type, word));
            i = j;
            continue;
        }

        // 5. Punctuation / Operators
        if matches!(
            ch,
            '=' | '+'
                | '-'
                | '*'
                | '/'
                | '%'
                | '<'
                | '>'
                | '!'
                | '&'
                | '|'
                | '^'
                | '~'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ','
                | ';'
                | ':'
                | '.'
        ) {
            tokens.push((TokenType::Punctuation, ch.to_string()));
            i += 1;
            continue;
        }

        // 6. Whitespace and other characters
        let mut j = i;
        while j < len
            && !chars[j].is_alphabetic()
            && !chars[j].is_ascii_digit()
            && chars[j] != '_'
            && chars[j] != '"'
            && chars[j] != '\''
            && chars[j] != '`'
            && !matches!(
                chars[j],
                '-' | '/'
                    | '#'
                    | '='
                    | '+'
                    | '*'
                    | '%'
                    | '<'
                    | '>'
                    | '!'
                    | '&'
                    | '|'
                    | '^'
                    | '~'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ','
                    | ';'
                    | ':'
                    | '.'
                    | '$'
            )
        {
            j += 1;
        }
        if j == i {
            tokens.push((TokenType::Normal, ch.to_string()));
            i += 1;
        } else {
            let other: String = chars[i..j].iter().collect();
            tokens.push((TokenType::Normal, other));
            i = j;
        }
    }

    tokens
}

/// Renders a single highlighted line into `out` with horizontal scroll slicing and active theme ANSI styles.
pub fn render_highlighted_line<W: std::io::Write>(
    line: &str,
    lang: Language,
    scroll_col: usize,
    max_cols: usize,
    theme: &Theme,
    out: &mut W,
) -> std::io::Result<()> {
    if lang == Language::Plain {
        let visible: String = line.chars().skip(scroll_col).take(max_cols).collect();
        write!(out, "{}{}\x1b[K\x1b[0m", theme.fg.to_fg_ansi(), visible)?;
        return Ok(());
    }

    let tokens = highlight_line(line, lang);
    let mut current_col = 0;
    let end_col = scroll_col.saturating_add(max_cols);
    let mut active_style: Option<TokenType> = None;

    for (token_type, text) in tokens {
        let char_count = text.chars().count();
        let token_end = current_col + char_count;

        if token_end <= scroll_col {
            current_col = token_end;
            continue;
        }

        if current_col >= end_col {
            break;
        }

        let skip_in_token = scroll_col.saturating_sub(current_col);
        let take_in_token = (end_col.saturating_sub(current_col + skip_in_token)).min(char_count - skip_in_token);

        let slice: String = text.chars().skip(skip_in_token).take(take_in_token).collect();

        if active_style != Some(token_type) {
            write!(out, "{}", token_type.ansi_color(theme))?;
            active_style = Some(token_type);
        }

        write!(out, "{}", slice)?;
        current_col = token_end;
    }

    write!(out, "\x1b[K\x1b[0m")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_highlighted_line() {
        let theme = Theme::kanagawa();
        let mut buf = Vec::new();
        render_highlighted_line("local x = 10", Language::Lua, 0, 50, &theme, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("local"));
        assert!(s.contains("10"));
        assert!(s.contains("\x1b[0m"));
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_path(Some(Path::new("lunula.lua"))), Language::Lua);
        assert_eq!(Language::from_path(Some(Path::new("src/main.rs"))), Language::Rust);
        assert_eq!(Language::from_path(Some(Path::new("profile.ps1"))), Language::Shell);
        assert_eq!(Language::from_path(Some(Path::new("Cargo.toml"))), Language::Config);
        assert_eq!(Language::from_path(Some(Path::new("script.py"))), Language::Python);
        assert_eq!(Language::from_path(Some(Path::new("README.md"))), Language::Markdown);
        assert_eq!(Language::from_path(None), Language::Plain);
    }

    #[test]
    fn test_lua_highlighting() {
        let tokens = highlight_line("local x = 42 -- comment", Language::Lua);
        assert_eq!(tokens[0], (TokenType::Keyword, "local".to_string()));
        assert_eq!(tokens[2], (TokenType::Normal, "x".to_string()));
        assert_eq!(tokens[4], (TokenType::Punctuation, "=".to_string()));
        assert_eq!(tokens[6], (TokenType::Number, "42".to_string()));
        assert_eq!(tokens[8], (TokenType::Comment, "-- comment".to_string()));
    }

    #[test]
    fn test_rust_highlighting() {
        let tokens = highlight_line("pub fn test() -> Option<u32> {", Language::Rust);
        assert_eq!(tokens[0], (TokenType::Keyword, "pub".to_string()));
        assert_eq!(tokens[2], (TokenType::Keyword, "fn".to_string()));
        assert_eq!(tokens[4], (TokenType::Normal, "test".to_string()));
        assert!(tokens.contains(&(TokenType::Type, "Option".to_string())));
        assert!(tokens.contains(&(TokenType::Type, "u32".to_string())));
    }
}
