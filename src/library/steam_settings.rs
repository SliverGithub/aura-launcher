//! Read and edit the per-game settings Steam stores in its own VDF files.
//!
//! * `userdata/<id>/config/localconfig.vdf` → `LaunchOptions`
//! * `config/config.vdf` → `CompatToolMapping` (the Proton/Wine tool)
//!
//! The editor is intentionally span-based: it only rewrites the key it is
//! asked to change and leaves the rest of Valve's file byte-for-byte intact.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SteamSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_options: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat_tool: Option<String>,
}

#[derive(Debug, Default)]
struct Node {
    key: Option<String>,
    value: Option<String>,
    /// Byte offset of `}` for blocks.
    close: usize,
    /// Byte span of the quoted key token.
    key_span: (usize, usize),
    /// Byte span of the quoted value token.
    value_span: Option<(usize, usize)>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, key: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|node| node.key.as_deref() == Some(key))
    }
}

struct Reader<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn bytes(&self) -> &'a [u8] {
        self.text.as_bytes()
    }

    fn skip_ws(&mut self) {
        loop {
            while self.pos < self.text.len() && self.bytes()[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.text[self.pos..].starts_with("//") {
                while self.pos < self.text.len() && self.bytes()[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn string(&mut self) -> Result<Option<(String, usize, usize)>> {
        self.skip_ws();
        if self.pos >= self.text.len() || self.bytes()[self.pos] != b'"' {
            return Ok(None);
        }
        let start = self.pos;
        self.pos += 1;
        let mut value = String::new();
        while self.pos < self.text.len() {
            match self.bytes()[self.pos] {
                b'"' => {
                    self.pos += 1;
                    return Ok(Some((value, start, self.pos)));
                }
                b'\\' if self.pos + 1 < self.text.len() => {
                    let next = self.bytes()[self.pos + 1];
                    match next {
                        b'n' => value.push('\n'),
                        b't' => value.push('\t'),
                        b'\\' => value.push('\\'),
                        b'"' => value.push('"'),
                        other => value.push(other as char),
                    }
                    self.pos += 2;
                }
                byte => {
                    // VDF files are UTF-8; find the next character boundary.
                    let ch = self.text[self.pos..].chars().next().unwrap_or('\u{fffd}');
                    let _ = byte;
                    value.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
        bail!("unterminated VDF string")
    }

    fn block(&mut self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.text.len() {
                break;
            }
            if self.bytes()[self.pos] == b'}' {
                break;
            }
            let Some((key, key_start, key_end)) = self.string()? else {
                bail!("expected a VDF key at byte {}", self.pos);
            };
            self.skip_ws();
            if self.pos < self.text.len() && self.bytes()[self.pos] == b'{' {
                self.pos += 1;
                let children = self.block()?;
                self.skip_ws();
                if self.pos >= self.text.len() || self.bytes()[self.pos] != b'}' {
                    bail!("unterminated VDF block {key:?}");
                }
                let close = self.pos;
                self.pos += 1;
                nodes.push(Node {
                    key: Some(key),
                    close,
                    key_span: (key_start, key_end),
                    children,
                    ..Default::default()
                });
            } else {
                let Some((value, value_start, value_end)) = self.string()? else {
                    bail!("expected a value for VDF key {key:?}");
                };
                nodes.push(Node {
                    key: Some(key),
                    value: Some(value),
                    key_span: (key_start, key_end),
                    value_span: Some((value_start, value_end)),
                    ..Default::default()
                });
            }
        }
        Ok(nodes)
    }
}

fn parse(text: &str) -> Result<Vec<Node>> {
    let mut reader = Reader::new(text);
    let nodes = reader.block()?;
    reader.skip_ws();
    if reader.pos < text.len() {
        bail!("unexpected trailing data in VDF at byte {}", reader.pos);
    }
    Ok(nodes)
}

fn find_path<'a>(nodes: &'a [Node], path: &[&str]) -> Option<&'a Node> {
    let (first, rest) = path.split_first()?;
    let node = nodes
        .iter()
        .find(|node| node.key.as_deref() == Some(*first))?;
    if rest.is_empty() {
        Some(node)
    } else {
        find_path(&node.children, rest)
    }
}

fn escaped(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn line_start(text: &str, pos: usize) -> usize {
    text[..pos].rfind('\n').map(|index| index + 1).unwrap_or(0)
}

fn line_end(text: &str, pos: usize) -> usize {
    text[pos..]
        .find('\n')
        .map(|index| pos + index + 1)
        .unwrap_or(text.len())
}

fn indent_at(text: &str, pos: usize) -> String {
    text[line_start(text, pos)..pos]
        .chars()
        .take_while(|ch| *ch == '\t' || *ch == ' ')
        .collect()
}

fn deeper(indent: &str) -> String {
    if indent.contains('\t') {
        format!("{indent}\t")
    } else {
        format!("{indent}    ")
    }
}

fn apply_edits(text: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|(start, _, _)| *start);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&text[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Resolve the two Steam config paths for the configured Steam root.
pub async fn paths(steam_root: &Path) -> Option<(PathBuf, PathBuf)> {
    let config = steam_root.join("config/config.vdf");
    let localconfig = localconfig_path(steam_root).await?;
    Some((localconfig, config))
}

async fn localconfig_path(steam_root: &Path) -> Option<PathBuf> {
    let userdata = steam_root.join("userdata");
    if let Some(id3) = crate::library::local_library::most_recent_account_id(steam_root).await {
        let candidate = userdata
            .join(id3.to_string())
            .join("config/localconfig.vdf");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let mut dir = tokio::fs::read_dir(&userdata).await.ok()?;
    while let Ok(Some(entry)) = dir.next_entry().await {
        let candidate = entry.path().join("config/localconfig.vdf");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub async fn read(steam_root: &Path, app_id: u32) -> Result<SteamSettings> {
    let Some((localconfig, config)) = paths(steam_root).await else {
        return Ok(SteamSettings::default());
    };
    let app = app_id.to_string();

    let launch_options = match tokio::fs::read_to_string(&localconfig).await {
        Ok(text) => parse(&text)
            .ok()
            .and_then(|nodes| {
                find_path(
                    &nodes,
                    &[
                        "UserLocalConfigStore",
                        "Software",
                        "Valve",
                        "Steam",
                        "apps",
                        &app,
                    ],
                )
                .and_then(|node| node.child("LaunchOptions"))
                .and_then(|node| node.value.clone())
            })
            .filter(|value| !value.is_empty()),
        Err(_) => None,
    };

    let compat_tool = match tokio::fs::read_to_string(&config).await {
        Ok(text) => parse(&text)
            .ok()
            .and_then(|nodes| {
                find_path(
                    &nodes,
                    &[
                        "InstallConfigStore",
                        "Software",
                        "Valve",
                        "Steam",
                        "CompatToolMapping",
                        &app,
                    ],
                )
                .and_then(|node| node.child("name"))
                .and_then(|node| node.value.clone())
            })
            .filter(|value| !value.is_empty()),
        Err(_) => None,
    };

    Ok(SteamSettings {
        launch_options,
        compat_tool,
    })
}

pub async fn write_launch_options(steam_root: &Path, app_id: u32, value: &str) -> Result<()> {
    let Some((localconfig, _)) = paths(steam_root).await else {
        bail!("Steam's localconfig.vdf was not found");
    };
    let text = tokio::fs::read_to_string(&localconfig)
        .await
        .with_context(|| format!("reading {}", localconfig.display()))?;
    let nodes = parse(&text).with_context(|| format!("parsing {}", localconfig.display()))?;
    let app = app_id.to_string();
    let apps = find_path(
        &nodes,
        &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"],
    )
    .context("Steam localconfig has no apps block")?;
    let new_value = format!("\"{}\"", escaped(value));

    let edits = if let Some(node) = apps.child(&app) {
        if let Some(option) = node.child("LaunchOptions") {
            let (start, end) = option.value_span.context("LaunchOptions is not a value")?;
            vec![(start, end, new_value)]
        } else {
            let close_line = line_start(&text, node.close);
            let indent = deeper(&indent_at(&text, node.close));
            vec![(
                close_line,
                close_line,
                format!("{indent}\"LaunchOptions\"\t\t{new_value}\n"),
            )]
        }
    } else {
        let close_line = line_start(&text, apps.close);
        let base = indent_at(&text, apps.close);
        let app_indent = deeper(&base);
        let inner = deeper(&app_indent);
        vec![(
            close_line,
            close_line,
            format!(
                "{app_indent}\"{app}\"\n{app_indent}{{\n{inner}\"LaunchOptions\"\t\t{new_value}\n{app_indent}}}\n"
            ),
        )]
    };
    write_atomic(&localconfig, &apply_edits(&text, edits)).await
}

pub async fn write_compat_tool(steam_root: &Path, app_id: u32, tool: Option<&str>) -> Result<()> {
    let Some((_, config)) = paths(steam_root).await else {
        bail!("Steam's config.vdf was not found");
    };
    let text = tokio::fs::read_to_string(&config)
        .await
        .with_context(|| format!("reading {}", config.display()))?;
    let nodes = parse(&text).with_context(|| format!("parsing {}", config.display()))?;
    let app = app_id.to_string();
    let mapping = find_path(
        &nodes,
        &[
            "InstallConfigStore",
            "Software",
            "Valve",
            "Steam",
            "CompatToolMapping",
        ],
    )
    .context("Steam config has no CompatToolMapping block")?;
    let new_value = tool
        .filter(|tool| !tool.trim().is_empty())
        .map(|tool| format!("\"{}\"", escaped(tool.trim())));

    let edits = if let Some(node) = mapping.child(&app) {
        match new_value {
            Some(value) => {
                if let Some(name) = node.child("name") {
                    let (start, end) =
                        name.value_span.context("compat tool name is not a value")?;
                    vec![(start, end, value)]
                } else {
                    let close_line = line_start(&text, node.close);
                    let indent = deeper(&indent_at(&text, node.close));
                    vec![(
                        close_line,
                        close_line,
                        format!("{indent}\"name\"\t\t{value}\n"),
                    )]
                }
            }
            None => {
                let start = line_start(&text, node.key_span.0);
                let end = line_end(&text, node.close);
                vec![(start, end, String::new())]
            }
        }
    } else if let Some(value) = new_value {
        let close_line = line_start(&text, mapping.close);
        let base = indent_at(&text, mapping.close);
        let app_indent = deeper(&base);
        let inner = deeper(&app_indent);
        vec![(
            close_line,
            close_line,
            format!(
                "{app_indent}\"{app}\"\n{app_indent}{{\n{inner}\"name\"\t\t{value}\n{inner}\"config\"\t\t\"\"\n{inner}\"priority\"\t\t\"250\"\n{app_indent}}}\n"
            ),
        )]
    } else {
        Vec::new()
    };
    if edits.is_empty() {
        return Ok(());
    }
    write_atomic(&config, &apply_edits(&text, edits)).await
}

async fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "vdf.tmp.{}.{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::write(&temp, contents)
        .await
        .with_context(|| format!("writing {}", temp.display()))?;
    tokio::fs::rename(&temp, path)
        .await
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: &str = r#""UserLocalConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"apps"
				{
					"730"
					{
						"Playtime"		"10"
						"LaunchOptions"		"-vulkan"
					}
					"999"
					{
						"Playtime"		"1"
					}
				}
			}
		}
	}
}"#;

    const CONFIG: &str = r#""InstallConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"CompatToolMapping"
				{
					"730"
					{
						"name"		"proton_10"
						"config"		""
						"priority"		"250"
					}
				}
			}
		}
	}
}"#;

    #[test]
    fn parses_paths_and_values() {
        let nodes = parse(LOCAL).unwrap();
        let app = find_path(
            &nodes,
            &[
                "UserLocalConfigStore",
                "Software",
                "Valve",
                "Steam",
                "apps",
                "730",
            ],
        )
        .unwrap();
        assert_eq!(
            app.child("LaunchOptions")
                .and_then(|node| node.value.as_deref()),
            Some("-vulkan")
        );
    }

    #[test]
    fn replaces_and_inserts_launch_options() {
        let text = LOCAL;
        let nodes = parse(text).unwrap();
        let apps = find_path(
            &nodes,
            &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"],
        )
        .unwrap();
        let app = apps.child("730").unwrap();
        let option = app.child("LaunchOptions").unwrap();
        let (start, end) = option.value_span.unwrap();
        let replaced = apply_edits(text, vec![(start, end, "\"prime-run %command%\"".into())]);
        assert!(replaced.contains("\"LaunchOptions\"\t\t\"prime-run %command%\""));
        assert!(replaced.contains("\"999\""));
    }

    #[test]
    fn inserts_new_app_block() {
        let text = LOCAL;
        let nodes = parse(text).unwrap();
        let apps = find_path(
            &nodes,
            &["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"],
        )
        .unwrap();
        // The helper logic in write_launch_options is span-based; just check the
        // insertion point produces a parseable file.
        let close_line = line_start(text, apps.close);
        let indent = deeper(&indent_at(text, apps.close));
        let inserted = apply_edits(
            text,
            vec![(
                close_line,
                close_line,
                format!(
                    "{indent}\"123\"\n{indent}{{\n{indent}\t\"LaunchOptions\"\t\t\"-x\"\n{indent}}}\n"
                ),
            )],
        );
        let nodes = parse(&inserted).unwrap();
        let app = find_path(
            &nodes,
            &[
                "UserLocalConfigStore",
                "Software",
                "Valve",
                "Steam",
                "apps",
                "123",
            ],
        )
        .unwrap();
        assert_eq!(
            app.child("LaunchOptions")
                .and_then(|node| node.value.as_deref()),
            Some("-x")
        );
    }

    #[tokio::test]
    async fn writes_and_reads_with_realistic_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("userdata/1/config")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("userdata/1/config/localconfig.vdf"), LOCAL).unwrap();
        std::fs::write(root.join("config/config.vdf"), CONFIG).unwrap();

        let initial = read(root, 730).await.unwrap();
        assert_eq!(initial.launch_options.as_deref(), Some("-vulkan"));
        assert_eq!(initial.compat_tool.as_deref(), Some("proton_10"));

        write_launch_options(root, 730, "prime-run %command%")
            .await
            .unwrap();
        write_compat_tool(root, 730, Some("proton_11"))
            .await
            .unwrap();
        write_launch_options(root, 620, "-novid").await.unwrap();
        write_compat_tool(root, 620, Some("proton_9"))
            .await
            .unwrap();

        let updated = read(root, 730).await.unwrap();
        assert_eq!(
            updated.launch_options.as_deref(),
            Some("prime-run %command%")
        );
        assert_eq!(updated.compat_tool.as_deref(), Some("proton_11"));
        let added = read(root, 620).await.unwrap();
        assert_eq!(added.launch_options.as_deref(), Some("-novid"));
        assert_eq!(added.compat_tool.as_deref(), Some("proton_9"));

        write_compat_tool(root, 620, None).await.unwrap();
        assert_eq!(read(root, 620).await.unwrap().compat_tool, None);
    }

    #[test]
    fn inserts_compat_mapping() {
        let text = CONFIG;
        let nodes = parse(text).unwrap();
        let mapping = find_path(
            &nodes,
            &[
                "InstallConfigStore",
                "Software",
                "Valve",
                "Steam",
                "CompatToolMapping",
            ],
        )
        .unwrap();
        let close_line = line_start(text, mapping.close);
        let indent = deeper(&indent_at(text, mapping.close));
        let inserted = apply_edits(
            text,
            vec![(
                close_line,
                close_line,
                format!(
                    "{indent}\"620\"\n{indent}{{\n{indent}\t\"name\"\t\t\"proton_11\"\n{indent}\t\"config\"\t\t\"\"\n{indent}\t\"priority\"\t\t\"250\"\n{indent}}}\n"
                ),
            )],
        );
        let nodes = parse(&inserted).unwrap();
        let app = find_path(
            &nodes,
            &[
                "InstallConfigStore",
                "Software",
                "Valve",
                "Steam",
                "CompatToolMapping",
                "620",
            ],
        )
        .unwrap();
        assert_eq!(
            app.child("name").and_then(|node| node.value.as_deref()),
            Some("proton_11")
        );
    }
}
