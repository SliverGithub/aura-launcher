//! Markdown/HTML-to-text rendering with a plain-text fallback.

use std::sync::OnceLock;

use ratatui::text::{Line, Span, Text};
use regex::Regex;

/// Render a description, news body, profile summary or Workshop text.
///
/// Steam store descriptions are HTML; Workshop/community text is Markdown.
/// HTML is flattened first, then the result is rendered as Markdown. If the
/// renderer produces nothing useful, the text is returned as plain lines.
pub fn render(input: &str) -> Text<'static> {
    let prepared = prepare(input);
    if prepared.trim().is_empty() {
        return Text::default();
    }
    let rendered = own(tui_markdown::from_str(&prepared));
    if rendered.lines.iter().all(line_is_empty) {
        plain(&prepared)
    } else {
        rendered
    }
}

/// Markdown-rendered lines, convenient for appending to a details pane.
pub fn lines(input: &str) -> Vec<Line<'static>> {
    render(input).lines
}

fn prepare(input: &str) -> String {
    let text = if looks_like_html(input) {
        html_to_text(input)
    } else {
        input.to_string()
    };
    bbcode_to_markdown(&text)
}

fn looks_like_html(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    [
        "<p", "<br", "<div", "<h1", "<h2", "<h3", "<li", "<ul", "<ol",
    ]
    .iter()
    .any(|tag| lower.contains(tag))
}

fn own(text: Text<'_>) -> Text<'static> {
    Text::from(
        text.lines
            .into_iter()
            .map(|line| Line {
                style: line.style,
                alignment: line.alignment,
                spans: line
                    .spans
                    .into_iter()
                    .map(|span| Span::styled(span.content.into_owned(), span.style))
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>(),
    )
}

fn line_is_empty(line: &Line) -> bool {
    line.spans.iter().all(|span| span.content.trim().is_empty())
}

fn plain(input: &str) -> Text<'static> {
    Text::from(
        input
            .lines()
            .map(|line| Line::from(strip_markdown(line)))
            .collect::<Vec<_>>(),
    )
}

/// Flatten HTML into text that the Markdown renderer understands.
pub fn html_to_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        if in_tag {
            if ch == '>' {
                apply_tag(&tag, &mut out);
                tag.clear();
                in_tag = false;
            } else {
                tag.push(ch);
            }
        } else if ch == '<' {
            in_tag = true;
        } else {
            out.push(ch);
        }
    }
    decode_entities(&out)
}

fn apply_tag(tag: &str, out: &mut String) {
    let trimmed = tag.trim();
    let closing = trimmed.starts_with('/');
    let name = trimmed
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    match name.as_str() {
        "br" => out.push('\n'),
        "p" | "div" | "ul" | "ol" | "table" | "tr" => blank_line(out),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            blank_line(out);
            if !closing {
                let level = name[1..].parse::<usize>().unwrap_or(1).clamp(1, 6);
                out.push_str(&"#".repeat(level));
                out.push(' ');
            }
        }
        "li" => {
            newline(out);
            if !closing {
                out.push_str("- ");
            }
        }
        "b" | "strong" => out.push_str("**"),
        "i" | "em" => out.push('*'),
        "code" => out.push('`'),
        _ => {}
    }
}

fn newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    newline(out);
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// Steam store descriptions mix HTML and BBCode. Convert the common BBCode
/// tags to Markdown so the renderer can style them, and drop media tags.
fn bbcode_to_markdown(input: &str) -> String {
    let mut out = input.to_string();
    out = cached_regex(r"(?is)\[img[^\]]*\].*?\[/img\]", "img")
        .replace_all(&out, "")
        .to_string();
    out = cached_regex(
        r"(?is)\[previewyoutube=[^\]]*\].*?\[/previewyoutube\]",
        "youtube",
    )
    .replace_all(&out, "")
    .to_string();
    out = cached_regex(r"(?is)\[url=[^\]]*\](.*?)\[/url\]", "url")
        .replace_all(&out, "$1")
        .to_string();
    out = cached_regex(r"(?is)\[url\](.*?)\[/url\]", "url2")
        .replace_all(&out, "$1")
        .to_string();
    out = cached_regex(r"(?i)\[/?(?:p|br)\]", "p")
        .replace_all(&out, "\n")
        .to_string();
    out = cached_regex(r"(?i)\[b\]", "b")
        .replace_all(&out, "**")
        .to_string();
    out = cached_regex(r"(?i)\[/b\]", "b2")
        .replace_all(&out, "**")
        .to_string();
    out = cached_regex(r"(?i)\[i\]", "i")
        .replace_all(&out, "*")
        .to_string();
    out = cached_regex(r"(?i)\[/i\]", "i2")
        .replace_all(&out, "*")
        .to_string();
    out = cached_regex(r"(?i)\[h([1-6])\]", "h")
        .replace_all(&out, "\n# ")
        .to_string();
    out = cached_regex(r"(?i)\[/h[1-6]\]", "h2")
        .replace_all(&out, "\n")
        .to_string();
    out = cached_regex(r"(?i)\[list\]", "list")
        .replace_all(&out, "\n")
        .to_string();
    out = cached_regex(r"(?i)\[/list\]", "list2")
        .replace_all(&out, "\n")
        .to_string();
    out = cached_regex(r"(?i)\[\*\]", "li")
        .replace_all(&out, "- ")
        .to_string();
    out = cached_regex(r"(?i)\[quote\]", "quote")
        .replace_all(&out, "> ")
        .to_string();
    out = cached_regex(r"(?i)\[/quote\]", "quote2")
        .replace_all(&out, "\n")
        .to_string();
    out = cached_regex(r"(?i)\[hr\]", "hr")
        .replace_all(&out, "\n---\n")
        .to_string();
    // Drop any remaining simple BBCode tags; keep their content.
    out = cached_regex(
        r"(?i)\[/?(?:u|code|noparse|spoiler|center|left|right|justify|table|tr|td|th|ol|ul|previewyoutube|img|url)(?:=[^\]]*)?\]",
        "leftover",
    ).replace_all(&out, "").to_string();
    out
}

fn cached_regex(pattern: &str, name: &str) -> &'static Regex {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static MAP: OnceLock<Mutex<HashMap<String, &'static Regex>>> = OnceLock::new();
    let map = MAP.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(regex) = guard.get(name) {
        return regex;
    }
    let leaked: &'static Regex = Box::leak(Box::new(Regex::new(pattern).unwrap()));
    guard.insert(name.to_string(), leaked);
    leaked
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

/// Minimal plain-text fallback for when a Markdown renderer is unavailable.
fn strip_markdown(line: &str) -> String {
    let mut text = line.trim_start_matches('#').trim_start().to_string();
    text = text.replace("**", "").replace("__", "");
    text = text.replace('`', "");
    text = text.replace(['*', '_'], "");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_store_html() {
        let html = "<h2>About</h2><p>Hello &amp; welcome<br>to <b>Steam</b>.</p><ul><li>One</li><li>Two</li></ul>";
        let text = html_to_text(html);
        assert!(text.contains("## About"));
        assert!(text.contains("Hello & welcome"));
        assert!(text.contains("to **Steam**."));
        assert!(text.contains("- One"));
        assert!(!text.contains("<p>"));
    }

    #[test]
    fn preserves_heading_line_style() {
        let text = render("## Heading");
        assert!(
            text.lines
                .iter()
                .any(|line| line.style != ratatui::style::Style::default()),
            "heading line style was dropped: {text:?}"
        );
    }

    #[test]
    fn applies_markdown_styles() {
        let text = render("## Heading\n\n**bold** and *italic*");
        let bold = text.lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("bold")
                && span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::BOLD)
        });
        let italic = text.lines.iter().flat_map(|line| &line.spans).any(|span| {
            span.content.contains("italic")
                && span
                    .style
                    .add_modifier
                    .contains(ratatui::style::Modifier::ITALIC)
        });
        assert!(bold, "bold span missing: {text:?}");
        assert!(italic, "italic span missing: {text:?}");
    }

    #[test]
    fn renders_markdown_without_losing_text() {
        let text = render("## Heading\n\n**bold** and *italic*");
        let joined: String = text
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(joined.contains("Heading"));
        assert!(joined.contains("bold"));
        assert!(joined.contains("italic"));
    }

    #[test]
    fn converts_steam_bbcode() {
        let text = render("[p]Hello[/p][h2]About[/h2][list][*]One[*]Two[/list]");
        let joined: String = text
            .lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!joined.contains("[p]"));
        assert!(!joined.contains("[h2]"));
        assert!(joined.contains("Hello"));
        assert!(joined.contains("About"));
        assert!(joined.contains("One"));
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(render("   ").lines.is_empty());
    }
}
