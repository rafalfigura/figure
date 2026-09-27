//! Reading an agent's shell command: which programs it runs, with which words.
//!
//! Not a shell: enough to tell `cat src/a.rs`, `sed -n 40,80p src/a.rs` and
//! `grep -rn Harm src` apart from everything else. When in doubt the guard lets a command
//! through, so this errs on the side of seeing less.

/// One simple command: a program and its words, quotes removed.
#[derive(Debug, PartialEq, Eq)]
pub struct Simple {
    /// Program name without its directory: `/usr/bin/grep` -> `grep`.
    pub program: String,
    pub args: Vec<String>,
    /// True when output of an earlier command is piped in, so it may read stdin.
    pub piped: bool,
}

/// The simple commands of `command`, in order. Heredoc bodies are skipped; `$(..)`,
/// `|`, `||`, `&&`, `;`, `&` and newlines separate commands; `VAR=value` prefixes and
/// `sudo`, `env`, `time`, `command` wrappers are dropped.
pub fn commands(command: &str) -> Vec<Simple> {
    let mut out = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word: Option<String> = None;
    let mut piped = false;
    let mut chars = without_heredocs(command)
        .chars()
        .collect::<Vec<_>>()
        .into_iter()
        .peekable();
    let mut end =
        |words: &mut Vec<String>, word: &mut Option<String>, next_piped: bool, piped: &mut bool| {
            words.extend(word.take());
            let mut rest = std::mem::take(words).into_iter().skip_while(|w| {
                w.contains('=') && !w.starts_with('-')
                    || ["sudo", "env", "time", "command"].contains(&w.as_str())
            });
            if let Some(program) = rest.next() {
                let program = program.rsplit('/').next().unwrap_or_default().to_string();
                out.push(Simple {
                    program,
                    args: rest.collect(),
                    piped: *piped,
                });
            }
            *piped = next_piped;
        };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                let w = word.get_or_insert_with(String::new);
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    w.push(q);
                }
            }
            '"' => {
                let w = word.get_or_insert_with(String::new);
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => w.extend(chars.next()),
                        _ => w.push(q),
                    }
                }
            }
            '\\' => word
                .get_or_insert_with(String::new)
                .extend(chars.next().filter(|n| *n != '\n')),
            '|' => {
                let pipe = chars.next_if_eq(&'|').is_none();
                end(&mut words, &mut word, pipe, &mut piped);
            }
            ';' | '&' | '\n' | '(' | ')' | '`' => {
                chars.next_if_eq(&'&');
                end(&mut words, &mut word, false, &mut piped);
            }
            '>' | '<' => {
                // A redirection: drop `2` in `2>`, then the operator and its target word.
                match word.take() {
                    Some(w) if w.chars().all(|d| d.is_ascii_digit()) => {}
                    w => words.extend(w),
                }
                while chars.next_if(|n| matches!(n, '>' | '<' | '&')).is_some() {}
                while chars.next_if(|n| *n == ' ').is_some() {}
                while chars
                    .next_if(|n| !n.is_whitespace() && !matches!(n, ';' | '|' | '&' | ')'))
                    .is_some()
                {}
            }
            '$' if chars.peek() == Some(&'(') => {}
            c if c.is_whitespace() => words.extend(word.take()),
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    end(&mut words, &mut word, false, &mut piped);
    out
}

/// `command` with the body of every heredoc (`<<EOF`, `<<-'EOF'`) removed.
fn without_heredocs(command: &str) -> String {
    let mut out = String::new();
    let mut until: Option<String> = None;
    for line in command.lines() {
        if let Some(end) = &until {
            if line.trim() == end {
                until = None;
            }
            continue;
        }
        if let Some(at) = line.find("<<").filter(|&at| !line[at..].starts_with("<<<")) {
            let tag: String = line[at + 2..]
                .trim_start_matches('-')
                .trim_start()
                .chars()
                .filter(|c| !matches!(c, '\'' | '"'))
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !tag.is_empty() {
                until = Some(tag);
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Positional words of a grep-like command, split into the pattern and the paths.
/// `takes_value` lists the short options that consume the next word.
pub fn pattern_and_paths(args: &[String], takes_value: &[&str]) -> (Option<String>, Vec<String>) {
    let mut pattern = None;
    let mut positional = Vec::new();
    let mut words = args.iter();
    while let Some(w) = words.next() {
        if w == "--" {
            positional.extend(words.by_ref().cloned());
        } else if w == "-e" || w == "--regexp" {
            pattern = words.next().cloned();
        } else if takes_value.contains(&w.as_str()) {
            words.next();
        } else if !w.starts_with('-') || w == "-" {
            positional.push(w.clone());
        }
    }
    if pattern.is_none() && !positional.is_empty() {
        pattern = Some(positional.remove(0));
    }
    (pattern, positional)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(c: &str) -> Vec<(String, Vec<String>, bool)> {
        commands(c)
            .into_iter()
            .map(|s| (s.program, s.args, s.piped))
            .collect()
    }

    /// Commands are split on pipes and separators, quotes are removed, pipes are marked.
    #[test]
    fn splits_and_unquotes() {
        let got = programs("cd /x && FOO=1 grep -rn 'fn run' src | head -5; cat \"a b.rs\"");
        assert_eq!(got[0].0, "cd");
        assert_eq!(
            got[1],
            (
                "grep".into(),
                vec!["-rn".into(), "fn run".into(), "src".into()],
                false
            )
        );
        assert_eq!(got[2], ("head".into(), vec!["-5".into()], true));
        assert_eq!(got[3], ("cat".into(), vec!["a b.rs".into()], false));
    }

    /// Redirections and their targets are not arguments, and do not split the command.
    #[test]
    fn drops_redirections() {
        let got = programs("grep -rn Harm src 2>&1 | head; cat a.rs 2> /dev/null");
        assert_eq!(got[0].1, ["-rn", "Harm", "src"]);
        assert_eq!(got[2].1, ["a.rs"]);
    }

    /// A heredoc body is data, not commands.
    #[test]
    fn skips_heredocs() {
        let got = programs("python3 - <<'EOF'\ncat src/a.rs\nEOF\nls");
        let names: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(names, ["python3", "ls"]);
    }

    /// `-e` names the pattern; options with values do not count as paths.
    #[test]
    fn grep_words() {
        let words: Vec<String> = ["-A", "3", "-e", "Harm", "src"].map(String::from).to_vec();
        assert_eq!(
            pattern_and_paths(&words, &["-A"]),
            (Some("Harm".into()), vec!["src".into()])
        );
    }
}
