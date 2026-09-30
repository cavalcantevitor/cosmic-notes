use chrono::Utc;
use cosmic_notes::core::{Note, WriteEchoCache};
use cosmic_notes::search::VaultIndex;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_zero_input_boundaries() {
    let index_dir = tempdir().expect("Failed to create index dir");
    let mut vault_index = VaultIndex::open_or_create(index_dir.path(), &[]).expect("Failed to create index");

    // 1. Completely empty note content (0 bytes)
    let empty_note = Note::parse(
        PathBuf::from("empty.md"),
        PathBuf::from("/tmp/empty.md"),
        String::new(),
        Utc::now(),
    )
    .expect("Should parse empty note gracefully");
    assert_eq!(empty_note.title, "empty");
    assert_eq!(empty_note.word_count(), 0);
    assert_eq!(empty_note.char_count(), 0);
    assert_eq!(empty_note.reading_time_mins(), 1);
    assert!(empty_note.tags.is_empty());
    assert!(empty_note.wikilinks.is_empty());

    // 2. Whitespace-only note
    let ws_note = Note::parse(
        PathBuf::from("whitespace.md"),
        PathBuf::from("/tmp/whitespace.md"),
        "   \n\t\n  \n".to_string(),
        Utc::now(),
    )
    .expect("Should parse whitespace note");
    assert_eq!(ws_note.word_count(), 0);

    // 3. Note with empty frontmatter
    let empty_fm = Note::parse(
        PathBuf::from("fm.md"),
        PathBuf::from("/tmp/fm.md"),
        "---\n---\n# Just Content\n".to_string(),
        Utc::now(),
    )
    .expect("Empty frontmatter should not error");
    assert_eq!(empty_fm.title, "Just Content");

    // 4. Index empty note and query with empty string
    vault_index.update_note(&empty_note).expect("Should index empty note");
    let fuzzy_empty = vault_index.quick_search("", 10);
    // Empty search should return all or none, without panicking
    assert!(fuzzy_empty.len() <= 1);

    let ft_empty = vault_index.fulltext_search("", 10).expect("Empty full-text search should return empty, not panic");
    assert!(ft_empty.is_empty());

    let ft_ws = vault_index.fulltext_search("   ", 10).expect("Whitespace full-text search should return empty, not panic");
    assert!(ft_ws.is_empty());
}

#[test]
fn test_malformed_and_special_char_boundaries() {
    let index_dir = tempdir().expect("Failed to create index dir");
    let mut vault_index = VaultIndex::open_or_create(index_dir.path(), &[]).expect("Failed to create index");

    // 1. Corrupted YAML frontmatter: must not panic, gracefully fall back to parsing body
    let corrupted_yaml = r#"---
title: "Unclosed title
tags: [broken,
  invalid_yaml: {unmatched
---
# Fallback Title

Body with broken frontmatter.
"#;
    let note = Note::parse(
        PathBuf::from("corrupted.md"),
        PathBuf::from("/tmp/corrupted.md"),
        corrupted_yaml.to_string(),
        Utc::now(),
    )
    .expect("Corrupted YAML should fall back safely");
    assert!(!note.title.is_empty(), "Note must retain a valid title fallback");

    // 2. Unclosed and malformed wikilinks
    let malformed_links = r#"# Broken Links
[[unclosed link without brackets
[[]]
[[|missing target]]
[[valid target|custom alias]]
"#;
    let link_note = Note::parse(
        PathBuf::from("links.md"),
        PathBuf::from("/tmp/links.md"),
        malformed_links.to_string(),
        Utc::now(),
    )
    .expect("Malformed links should not crash");
    // Only the valid target should be extracted
    assert_eq!(link_note.wikilinks.len(), 1);
    assert_eq!(link_note.wikilinks[0].target, "valid target");
    assert_eq!(link_note.wikilinks[0].display, Some("custom alias".to_string()));

    // 3. Search query injection strings with special Tantivy/Lucene operators
    // Must handle special characters without panicking
    vault_index.update_note(&link_note).expect("Index link note");
    let injection_queries = [
        "AND OR NOT",
        "+++ ---",
        "&& || !",
        "( ) { } [ ]",
        "^ \" ~ * ? : \\ /",
        "<script>alert('xss')</script>",
        "'; DROP TABLE notes; --",
    ];

    for query in &injection_queries {
        let result = vault_index.fulltext_search(query, 10);
        assert!(result.is_ok(), "Query '{}' must not panic or cause error: {:?}", query, result.err());
    }

    // 4. Unicode, accents and emojis
    let unicode_content = r#"---
title: "Notas de Pesquisa 🚀"
tags: [inovação, inteligência]
---
# Visão Geral

Desenvolvimento em Rust para a plataforma Pop!_OS e COSMIC.
Veja [[Arquitetura Estratégica]].
"#;
    let unicode_note = Note::parse(
        PathBuf::from("notas_pesquisa_🚀.md"),
        PathBuf::from("/tmp/notas_pesquisa_🚀.md"),
        unicode_content.to_string(),
        Utc::now(),
    )
    .expect("Unicode parsing failed");
    assert_eq!(unicode_note.title, "Notas de Pesquisa 🚀");
    assert!(unicode_note.tags.contains(&"inovação".to_string()));
    assert_eq!(unicode_note.wikilinks[0].target, "Arquitetura Estratégica");

    vault_index.update_note(&unicode_note).expect("Failed to index unicode note");
    let match_accent = vault_index.quick_search("inovação", 10);
    assert!(!match_accent.is_empty());
    let match_emoji = vault_index.quick_search("🚀", 10);
    assert!(!match_emoji.is_empty());
}

#[test]
fn test_excessive_payload_and_concurrency_stress() {
    let index_dir = tempdir().expect("Failed to create index dir");
    let mut vault_index = VaultIndex::open_or_create(index_dir.path(), &[]).expect("Failed to create index");

    // 1. Large document: 10,000 lines
    let mut large_text = String::with_capacity(500_000);
    large_text.push_str("# Big Note\n\n");
    for i in 0..10_000 {
        large_text.push_str(&format!("Line {}: Quick brown fox jumps over the lazy dog.\n", i));
    }
    let large_note = Note::parse(
        PathBuf::from("big.md"),
        PathBuf::from("/tmp/big.md"),
        large_text,
        Utc::now(),
    )
    .expect("Large note parsing failed");
    assert_eq!(large_note.word_count(), 100_003);
    assert!(large_note.reading_time_mins() >= 400);

    vault_index.update_note(&large_note).expect("Large note indexing failed");
    let search_res = vault_index.fulltext_search("brown fox", 10).expect("Search in large note failed");
    assert_eq!(search_res.len(), 1);

    // 2. Note with 100+ tags and 100+ wikilinks
    let mut many_tags_text = String::from("# Many Tags\n\n");
    for i in 0..150 {
        many_tags_text.push_str(&format!("#tag_{} ", i));
        many_tags_text.push_str(&format!("[[Target Note {}]] ", i));
    }
    let many_note = Note::parse(
        PathBuf::from("many.md"),
        PathBuf::from("/tmp/many.md"),
        many_tags_text,
        Utc::now(),
    )
    .expect("Many note parse failed");
    assert_eq!(many_note.tags.len(), 150);
    assert_eq!(many_note.wikilinks.len(), 150);

    // 3. Write-Echo cache high concurrency burst
    let cache = WriteEchoCache::new(Duration::from_millis(500));
    for i in 0..100 {
        let p = format!("note_{}.md", i);
        cache.register(Path::new(&p), None);
    }
    for i in 0..100 {
        let p = format!("note_{}.md", i);
        assert!(cache.consume_echo(Path::new(&p), None));
    }
}
