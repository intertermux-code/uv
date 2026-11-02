use crate::{Shell, Simplified};
use std::borrow::Cow;
use std::path::Path;

/// Quote a path, if necessary, for safe use in a POSIX-compatible shell command.
pub fn shlex_posix(executable: impl AsRef<Path>) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().portable_display().to_string();

    // Match Python's `shlex.quote` and leave only shell-safe ASCII characters unquoted.
    if !executable.is_empty()
        && executable
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@%+=:,./-_".contains(&byte))
    {
        executable
    } else {
        format!("'{}'", escape_posix_for_single_quotes(&executable))
    }
}

/// Escape a string for being used in single quotes in a POSIX-compatible shell command.
///
/// We want our scripts to support any POSIX shell. There are two kinds of quotes in POSIX:
/// Single and double quotes. In bash, single quotes must not contain another single
/// quote, you can't even escape it (<https://linux.die.net/man/1/bash> under "QUOTING").
/// Double quotes have escaping rules that differ from shell to shell, which we can't handle.
/// Bash has `$'\''`, but that's not universal enough.
///
/// As a solution, use implicit string concatenations, by putting the single quote into double
/// quotes.
pub fn escape_posix_for_single_quotes(string: &str) -> Cow<'_, str> {
    if string.contains('\'') {
        Cow::Owned(string.replace('\'', r#"'"'"'"#))
    } else {
        Cow::Borrowed(string)
    }
}

/// Escape a string for being used in single quotes in a `fish` command.
///
/// `fish` single quotes are not POSIX single quotes: inside `'...'`, `fish` honors `\'` and `\\`
/// as escapes and treats every other character literally, so a single quote cannot be quoted out
/// of the string the way it can in a POSIX shell. Escaping the backslash first, then the quote,
/// keeps a value like `x\'$(touch PWNED)'` from ending the literal early.
pub fn escape_fish_for_single_quotes(string: &str) -> String {
    string.replace('\\', r"\\").replace('\'', r"\'")
}

/// Escape a string for being used in single quotes in a `csh` or `tcsh` command.
///
/// Like POSIX shells, `csh` gives no way to escape a single quote inside a single-quoted string, so
/// the quote is spliced in through a double-quoted segment. `csh` additionally performs history
/// substitution before it removes quotes, so a `!` is substituted even inside single quotes and has
/// to be preceded by a backslash, which the substitution consumes.
pub fn escape_csh_for_single_quotes(string: &str) -> Cow<'_, str> {
    if string.contains('\'') || string.contains('!') {
        Cow::Owned(string.replace('!', r"\!").replace('\'', r#"'"'"'"#))
    } else {
        Cow::Borrowed(string)
    }
}

/// Escape a string for being used in double quotes in a Nushell command.
///
/// Nushell accepts a fixed set of backslash escapes inside double quotes and rejects any other as
/// an invalid escape sequence, so only the backslash and the closing quote can be escaped. Unlike
/// in a shell like bash, `$` and backticks are not substituted inside double quotes.
pub fn escape_nushell_for_double_quotes(string: &str) -> Cow<'_, str> {
    if string.contains('\\') || string.contains('"') {
        Cow::Owned(string.replace('\\', r"\\").replace('"', r#"\""#))
    } else {
        Cow::Borrowed(string)
    }
}

/// Escape a string for being used in single quotes in a PowerShell command.
///
/// PowerShell has no escape character inside single quotes and writes a literal quote by doubling
/// it; every other character, including `$` and the backtick, stays literal.
pub fn escape_powershell_for_single_quotes(string: &str) -> Cow<'_, str> {
    if string.contains('\'') {
        Cow::Owned(string.replace('\'', "''"))
    } else {
        Cow::Borrowed(string)
    }
}

/// Escape a string for being used in double quotes in a Python string literal.
///
/// The literal is a `str`, so characters are escaped the way Python reads them: the backslash and
/// the closing quote are escaped in place, the short escape sequences cover the whitespace Python
/// ends a line on, and the remaining control characters are written as `\u` escapes.
pub fn escape_python_for_double_quotes(string: &str) -> String {
    use std::fmt::Write;

    let mut escaped = String::with_capacity(string.len());
    for character in string.chars() {
        match character {
            '\\' => escaped.push_str(r"\\"),
            '"' => escaped.push_str(r#"\""#),
            '\n' => escaped.push_str(r"\n"),
            '\r' => escaped.push_str(r"\r"),
            '\t' => escaped.push_str(r"\t"),
            other if other.is_control() => {
                let code = u32::from(other);
                let _ = write!(escaped, r"\u{code:04x}");
            }
            other => escaped.push(other),
        }
    }
    escaped
}

/// Escape a string for being used in a quoted `set` assignment in a `cmd` batch file.
///
/// `cmd` expands percent variables before it parses the line, so a percent sign in the value can
/// pull text that is not quoted into the line. Percent signs are therefore doubled, which is how a
/// batch file spells a literal percent. The remaining replacements drop the characters that cannot
/// appear in the assignment at all: a double quote ends the protection the quotes around the
/// assignment provide, leaving anything after it parsed as commands, and a newline ends the line.
pub fn escape_cmd_for_quoted_set(string: &str) -> Cow<'_, str> {
    if string.contains('%') || string.contains(['"', '\n', '\r']) {
        Cow::Owned(
            string
                .replace('%', "%%")
                .chars()
                .filter(|character| !matches!(character, '"' | '\n' | '\r'))
                .collect(),
        )
    } else {
        Cow::Borrowed(string)
    }
}

/// Quote a path, if necessary, for safe use in `PowerShell` and `cmd`.
pub fn shlex_windows(executable: impl AsRef<Path>, shell: Shell) -> String {
    // Convert to a display path.
    let executable = executable.as_ref().user_display().to_string();

    // Wrap the executable in quotes (and a `&` invocation on PowerShell), if it contains spaces.
    if executable.contains(' ') {
        if shell == Shell::Powershell {
            // For PowerShell, wrap in a `&` invocation.
            format!("& \"{executable}\"")
        } else {
            // Otherwise, assume `cmd`, which doesn't need the `&`.
            format!("\"{executable}\"")
        }
    } else {
        executable
    }
}

#[cfg(test)]
mod tests {
    use super::{
        escape_cmd_for_quoted_set, escape_csh_for_single_quotes, escape_fish_for_single_quotes,
        escape_nushell_for_double_quotes, escape_posix_for_single_quotes,
        escape_powershell_for_single_quotes, escape_python_for_double_quotes, shlex_posix,
    };

    #[test]
    fn fish_plain_value() {
        assert_eq!(escape_fish_for_single_quotes("venv"), "venv");
    }

    #[test]
    fn fish_quote_ends_the_literal() {
        assert_eq!(
            escape_fish_for_single_quotes(r"x'; touch /tmp/PWNED; set x '"),
            r"x\'; touch /tmp/PWNED; set x \'"
        );
    }

    #[test]
    fn fish_backslash_before_quote() {
        // A backslash directly before the quote would otherwise pair up with it and leave the
        // closing quote unconsumed.
        assert_eq!(escape_fish_for_single_quotes(r"x\'"), r"x\\\'");
    }

    #[test]
    fn posix_quote_ends_the_literal() {
        assert_eq!(
            escape_posix_for_single_quotes(r"x'; touch /tmp/PWNED; set x '"),
            r#"x'"'"'; touch /tmp/PWNED; set x '"'"'"#
        );
    }

    #[test]
    fn posix_safe_path() {
        assert_eq!(shlex_posix("/usr/bin/python3.12"), "/usr/bin/python3.12");
    }

    #[test]
    fn csh_plain_value() {
        assert_eq!(escape_csh_for_single_quotes("venv"), "venv");
    }

    #[test]
    fn csh_quote_ends_the_literal() {
        assert_eq!(
            escape_csh_for_single_quotes(r"x'; touch /tmp/PWNED; set x '"),
            r#"x'"'"'; touch /tmp/PWNED; set x '"'"'"#
        );
    }

    #[test]
    fn csh_history_substitution_is_inhibited() {
        assert_eq!(escape_csh_for_single_quotes("a!b"), r"a\!b");
    }

    #[test]
    fn nushell_plain_value() {
        assert_eq!(escape_nushell_for_double_quotes("venv"), "venv");
    }

    #[test]
    fn nushell_quote_ends_the_literal() {
        assert_eq!(
            escape_nushell_for_double_quotes(r#"x"; touch /tmp/PWNED; set x"#),
            r#"x\"; touch /tmp/PWNED; set x"#
        );
    }

    #[test]
    fn nushell_backslash_is_escaped_first() {
        // Escaping the quote first would leave the backslash escaping the quote it precedes.
        assert_eq!(escape_nushell_for_double_quotes("x\\"), r"x\\");
    }

    #[test]
    fn powershell_plain_value() {
        assert_eq!(escape_powershell_for_single_quotes("venv"), "venv");
    }

    #[test]
    fn powershell_quote_is_doubled() {
        assert_eq!(
            escape_powershell_for_single_quotes("x'; Set-Content PWNED; '"),
            "x''; Set-Content PWNED; ''"
        );
    }

    #[test]
    fn python_quote_ends_the_literal() {
        assert_eq!(
            escape_python_for_double_quotes("x\"; import os; os.system(\"touch /tmp/PWNED\")"),
            r#"x\"; import os; os.system(\"touch /tmp/PWNED\")"#
        );
    }

    #[test]
    fn python_control_characters_are_escaped() {
        assert_eq!(
            escape_python_for_double_quotes("a\\\n\t\u{1b}b"),
            r"a\\\n\t\u001bb"
        );
    }

    #[test]
    fn cmd_percent_is_doubled_and_quote_is_dropped() {
        assert_eq!(
            escape_cmd_for_quoted_set("%PROMPT%\"&calc"),
            r"%%PROMPT%%&calc"
        );
    }

    #[test]
    fn cmd_newline_is_dropped() {
        assert_eq!(escape_cmd_for_quoted_set("a\nb\r\nc"), "abc");
    }

    #[test]
    fn cmd_plain_value() {
        assert_eq!(escape_cmd_for_quoted_set("venv"), "venv");
    }

    #[test]
    fn posix_empty_path() {
        assert_eq!(shlex_posix(""), "''");
    }

    #[test]
    fn posix_path_with_metacharacters() {
        assert_eq!(
            shlex_posix("Testing's/$venv;activate"),
            r#"'Testing'"'"'s/$venv;activate'"#
        );
    }
}
