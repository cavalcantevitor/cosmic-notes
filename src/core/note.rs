use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::core::error::{Result, VaultError};

/// Parsed YAML Frontmatter at the top of a Markdown note.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frontmatter {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_tags")]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_tags")]
    pub aliases: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

// Allows `tags: tag1` or `tags: [tag1, tag2]`
fn deserialize_tags<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    match Option::<StringOrVec>::deserialize(deserializer)? {
        Some(StringOrVec::String(s)) => Ok(vec![s]),
        Some(StringOrVec::Vec(v)) => Ok(v),
        None => Ok(Vec::new()),
    }
}

/// A parsed wikilink reference: `[[target]]` or `[[target|display_text]]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wikilink {
    pub target: String,
    pub display: Option<String>,
}

/// A local-first note document residing in the filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Relative path within the vault (e.g. `ideas/project.md`)
    pub path: PathBuf,
    /// Absolute filesystem path
    pub absolute_path: PathBuf,
    /// Note title (derived from frontmatter, H1 header, or filename)
    pub title: String,
    /// Raw unparsed file content
    pub raw_content: String,
    /// Markdown body without frontmatter
    pub body: String,
    /// Parsed frontmatter if present
    pub frontmatter: Option<Frontmatter>,
    /// Combined deduplicated tags (frontmatter + inline `#tags`)
    pub tags: Vec<String>,
    /// Extracted wikilinks (`[[link]]`)
    pub wikilinks: Vec<Wikilink>,
    /// Last modified time
    pub modified_at: DateTime<Utc>,
    /// Content hash for lazy rendering and echo checking
    pub content_hash: u64,
}

impl Note {
    /// Parse a note from raw content and paths.
    pub fn parse(
        relative_path: PathBuf,
        absolute_path: PathBuf,
        raw_content: String,
        modified_at: DateTime<Utc>,
    ) -> Result<Self> {
        let (frontmatter, body) = Self::extract_frontmatter(&raw_content, &relative_path)?;

        let mut tags_set = BTreeSet::new();

        // Add frontmatter tags
        if let Some(ref fm) = frontmatter {
            for tag in &fm.tags {
                tags_set.insert(clean_tag(tag));
            }
        }

        // Add inline tags from body
        for tag in extract_inline_tags(&body) {
            tags_set.insert(tag);
        }

        let tags: Vec<String> = tags_set.into_iter().collect();
        let wikilinks = extract_wikilinks(&body);

        let title = if let Some(fm_title) = frontmatter.as_ref().and_then(|f| f.title.clone()) {
            fm_title
        } else if let Some(h1_title) = extract_first_h1(&body) {
            h1_title
        } else {
            relative_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        };

        let content_hash = calculate_content_hash(&raw_content);

        Ok(Self {
            path: relative_path,
            absolute_path,
            title,
            raw_content,
            body,
            frontmatter,
            tags,
            wikilinks,
            modified_at,
            content_hash,
        })
    }

    /// Extract YAML frontmatter block if present.
    fn extract_frontmatter(content: &str, path: &Path) -> Result<(Option<Frontmatter>, String)> {
        // Frontmatter must begin on the first line with `---`
        if !content.starts_with("---") {
            return Ok((None, content.to_string()));
        }

        // Check if there is a closing `---`
        let rest = &content[3..];
        if !rest.starts_with('\n') && !rest.starts_with("\r\n") {
            return Ok((None, content.to_string()));
        }

        let newline_offset = if rest.starts_with("\r\n") { 2 } else { 1 };
        let yaml_candidate = &rest[newline_offset..];

        // Search for closing `\n---` or `\r\n---`
        let end_idx = find_closing_delimiter(yaml_candidate);

        if let Some(idx) = end_idx {
            let yaml_str = &yaml_candidate[..idx];
            let after_closing = &yaml_candidate[idx..];

            // Strip the closing `---` and any immediate newline
            let body_start = if after_closing.starts_with("\r\n---\r\n") {
                7
            } else if after_closing.starts_with("\r\n---\n") {
                6
            } else if after_closing.starts_with("\n---\r\n") {
                6
            } else if after_closing.starts_with("\n---\n") {
                5
            } else if after_closing.starts_with("\n---") || after_closing.starts_with("\r\n---") {
                after_closing.len()
            } else {
                0
            };

            let body = if body_start < after_closing.len() {
                after_closing[body_start..].to_string()
            } else {
                String::new()
            };

            let fm: Frontmatter = serde_yaml::from_str(yaml_str).map_err(|e| {
                VaultError::FrontmatterParse {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                }
            })?;

            Ok((Some(fm), body))
        } else {
            Ok((None, content.to_string()))
        }
    }

    /// Count words in the note body.
    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }

    /// Count characters in the note body.
    pub fn char_count(&self) -> usize {
        self.body.chars().count()
    }

    /// Estimated reading time in minutes (assuming 200 WPM).
    pub fn reading_time_mins(&self) -> usize {
        let words = self.word_count();
        (words / 200).max(1)
    }

    /// Extract a clean 1-line excerpt for note list cards.
    pub fn excerpt(&self, max_chars: usize) -> String {
        for line in self.body.lines() {
            let trimmed = line.trim();
            // Skip empty lines, headings, frontmatter/code blocks, or tag lines
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with("```")
                || trimmed.starts_with("---")
            {
                continue;
            }

            // Strip leading list bullets, blockquotes, or wikilink brackets
            let cleaned = trimmed
                .trim_start_matches(|c| c == '-' || c == '*' || c == '>' || c == ' ')
                .replace("[[", "")
                .replace("]]", "");

            if cleaned.is_empty() {
                continue;
            }

            if cleaned.chars().count() > max_chars {
                let truncated: String = cleaned.chars().take(max_chars).collect();
                return format!("{truncated}…");
            } else {
                return cleaned;
            }
        }
        "No additional text".to_string()
    }

    /// Format modified date cleanly (e.g. "Sep 29" or "HH:MM").
    pub fn formatted_date(&self) -> String {
        let now = Utc::now();
        if self.modified_at.date_naive() == now.date_naive() {
            self.modified_at.format("%H:%M").to_string()
        } else {
            self.modified_at.format("%b %d").to_string()
        }
    }
}


pub fn calculate_content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn clean_tag(tag: &str) -> String {
    let trimmed = tag.trim();
    if let Some(stripped) = trimmed.strip_prefix('#') {
        stripped.to_lowercase()
    } else {
        trimmed.to_lowercase()
    }
}

fn find_closing_delimiter(s: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(pos) = s[offset..].find("---") {
        let abs_pos = offset + pos;
        let before_ok = abs_pos == 0
            || s[..abs_pos].ends_with('\n')
            || s[..abs_pos].ends_with("\r\n");
        let after = &s[abs_pos + 3..];
        let after_ok = after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n");

        if before_ok && after_ok {
            // Find index of the newline preceding `---` if any
            if abs_pos >= 2 && &s[abs_pos - 2..abs_pos] == "\r\n" {
                return Some(abs_pos - 2);
            } else if abs_pos >= 1 && s.as_bytes()[abs_pos - 1] == b'\n' {
                return Some(abs_pos - 1);
            } else {
                return Some(abs_pos);
            }
        }
        offset = abs_pos + 3;
    }
    None
}

/// Extract first H1 heading (`# Title`) if present in the Markdown body.
fn extract_first_h1(body: &str) -> Option<String> {
    let mut in_code_block = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if let Some(heading) = trimmed.strip_prefix("# ") {
            let title = heading.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// Extract inline `#tags` from the Markdown body while avoiding headers and code blocks.
pub fn extract_inline_tags(body: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut in_code_block = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        // Skip headers (lines starting with # followed by a space)
        if trimmed.starts_with('#') && trimmed[1..].starts_with(' ') {
            continue;
        }

        let words = line.split_whitespace();
        for word in words {
            // Must start with `#` and have characters after it
            if let Some(tag_candidate) = word.strip_prefix('#') {
                if !tag_candidate.is_empty()
                    && !tag_candidate.starts_with('#')
                    && !tag_candidate.chars().next().map_or(false, |c| c.is_ascii_punctuation())
                {
                    // Clean trailing punctuation
                    let cleaned = tag_candidate
                        .trim_matches(|c: char| c.is_ascii_punctuation() && c != '/' && c != '_' && c != '-');
                    if !cleaned.is_empty() {
                        tags.push(cleaned.to_lowercase());
                    }
                }
            }
        }
    }

    tags
}

/// Extract `[[target]]` or `[[target|display]]` wikilinks.
pub fn extract_wikilinks(body: &str) -> Vec<Wikilink> {
    let mut links = Vec::new();
    let mut in_code_block = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        let mut remainder = line;
        while let Some(start) = remainder.find("[[") {
            let after_start = &remainder[start + 2..];
            if let Some(end) = after_start.find("]]") {
                let inside = &after_start[..end];
                if !inside.trim().is_empty() {
                    let (target, display) = if let Some(pipe_idx) = inside.find('|') {
                        let target = inside[..pipe_idx].trim().to_string();
                        let display = inside[pipe_idx + 1..].trim().to_string();
                        (target, Some(display))
                    } else {
                        (inside.trim().to_string(), None)
                    };

                    if !target.is_empty() {
                        links.push(Wikilink { target, display });
                    }
                }
                remainder = &after_start[end + 2..];
            } else {
                break;
            }
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_with_frontmatter_and_wikilinks() {
        let raw = r#"---
title: System Architecture
tags:
  - rust
  - cosmic
aliases: [Architecture, Design]
---

# System Architecture

Here is a note linking to [[Getting Started|Start Here]] and [[Overview]].

Don't forget #workflow/automation and #performance.

```rust
// This is not a #tag
```
"#;

        let note = Note::parse(
            PathBuf::from("architecture.md"),
            PathBuf::from("/vault/architecture.md"),
            raw.to_string(),
            Utc::now(),
        )
        .expect("Failed to parse note");

        assert_eq!(note.title, "System Architecture");
        assert!(note.tags.contains(&"rust".to_string()));
        assert!(note.tags.contains(&"cosmic".to_string()));
        assert!(note.tags.contains(&"workflow/automation".to_string()));
        assert!(note.tags.contains(&"performance".to_string()));
        assert!(!note.tags.contains(&"tag".to_string())); // ignored inside code block

        assert_eq!(note.wikilinks.len(), 2);
        assert_eq!(
            note.wikilinks[0],
            Wikilink {
                target: "Getting Started".to_string(),
                display: Some("Start Here".to_string()),
            }
        );
        assert_eq!(
            note.wikilinks[1],
            Wikilink {
                target: "Overview".to_string(),
                display: None,
            }
        );
    }

    #[test]
    fn test_parse_note_without_frontmatter_uses_h1() {
        let raw = "# My First Note\n\nSome great thoughts.";
        let note = Note::parse(
            PathBuf::from("notes/first.md"),
            PathBuf::from("/vault/notes/first.md"),
            raw.to_string(),
            Utc::now(),
        )
        .unwrap();

        assert_eq!(note.title, "My First Note");
        assert!(note.frontmatter.is_none());
    }

    #[test]
    fn test_parse_note_fallback_to_filename() {
        let raw = "Just plain thoughts without any header.";
        let note = Note::parse(
            PathBuf::from("random_thoughts.md"),
            PathBuf::from("/vault/random_thoughts.md"),
            raw.to_string(),
            Utc::now(),
        )
        .unwrap();

        assert_eq!(note.title, "random_thoughts");
    }

    #[test]
    fn test_note_telemetry_and_excerpt() {
        let raw = "# Note Title\n\n- [[Target Link]] This is the first sentence for preview.\nAnd here is a second line.";
        let note = Note::parse(
            PathBuf::from("test.md"),
            PathBuf::from("/vault/test.md"),
            raw.to_string(),
            Utc::now(),
        )
        .unwrap();

        assert_eq!(note.word_count(), 19);
        assert_eq!(note.reading_time_mins(), 1);

        let excerpt = note.excerpt(30);
        assert!(excerpt.starts_with("Target Link This is the"));
        assert!(excerpt.ends_with('…'));
    }
}

