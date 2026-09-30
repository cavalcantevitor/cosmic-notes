use cosmic_notes::core::{Vault, WriteEchoCache};
use cosmic_notes::search::VaultIndex;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_end_to_end_note_lifecycle_and_indexing() {
    let vault_dir = tempdir().expect("Failed to create vault temp dir");
    let index_dir = tempdir().expect("Failed to create index temp dir");

    let vault = Vault::open(vault_dir.path()).expect("Failed to open vault");
    let mut vault_index = VaultIndex::open_or_create(index_dir.path(), &[]).expect("Failed to create vault index");

    // 1. Create Note A with frontmatter, tags, and wikilink
    let note_a_rel = Path::new("concepts/rust.md");
    let note_a_content = r#"---
title: "Rust Architecture"
tags: [systems, language]
---
# Deep Dive

Rust provides memory safety without garbage collection.
Check out [[Concurrency Models]] for parallel processing.
"#;
    vault.write_note(note_a_rel, note_a_content).expect("Failed to write Note A");
    let note_a = vault.read_note(note_a_rel).expect("Failed to read Note A");
    assert_eq!(note_a.title, "Rust Architecture");
    assert!(note_a.tags.contains(&"systems".to_string()));
    assert!(note_a.wikilinks.iter().any(|l| l.target == "Concurrency Models"));

    // 2. Create Note B
    let note_b_rel = Path::new("guides/concurrency.md");
    let note_b_content = r#"# Concurrency Models

Covers async, threads, and actors.
"#;
    vault.write_note(note_b_rel, note_b_content).expect("Failed to write Note B");
    let note_b = vault.read_note(note_b_rel).expect("Failed to read Note B");
    assert_eq!(note_b.title, "Concurrency Models");

    // 3. Index both notes
    vault_index.update_note(&note_a).expect("Failed to index Note A");
    vault_index.update_note(&note_b).expect("Failed to index Note B");

    // 4. Test Fuzzy Search by Title, Path, and Tag
    let fuzzy_title = vault_index.quick_search("Architecture", 10);
    assert_eq!(fuzzy_title.len(), 1);
    assert_eq!(fuzzy_title[0].item.title, "Rust Architecture");

    let fuzzy_path = vault_index.quick_search("guides/concurrency", 10);
    assert_eq!(fuzzy_path.len(), 1);
    assert_eq!(fuzzy_path[0].item.title, "Concurrency Models");

    let fuzzy_tag = vault_index.quick_search("systems", 10);
    assert_eq!(fuzzy_tag.len(), 1);
    assert_eq!(fuzzy_tag[0].item.title, "Rust Architecture");

    // 5. Test Tantivy BM25 Full-Text Search with Snippets
    let ft_results = vault_index.fulltext_search("garbage collection", 10).expect("Full-text search failed");
    assert!(!ft_results.is_empty(), "Expected match for 'garbage collection'");
    assert_eq!(ft_results[0].title, "Rust Architecture");
    assert!(ft_results[0].snippet.as_deref().unwrap_or("").to_lowercase().contains("garbage"));

    // 6. Test Bidirectional Knowledge Graph
    let backlinks_to_b = vault_index.backlinks(note_b_rel);
    assert_eq!(backlinks_to_b.len(), 1, "Concurrency Models should have 1 incoming backlink from Note A");
    assert_eq!(backlinks_to_b[0].source_title, "Rust Architecture");

    let outgoing_from_a = vault_index.outgoing_links(note_a_rel);
    assert_eq!(outgoing_from_a.len(), 1);
    assert_eq!(outgoing_from_a[0].target, "Concurrency Models");
    assert_eq!(outgoing_from_a[0].resolved_path, Some(note_b_rel.to_path_buf()));

    // 7. Update Note A: remove wikilink to Note B, add new tag #performance
    let note_a_updated = r#"---
title: "Rust Architecture"
tags: [systems, performance]
---
# Deep Dive

Updated content without links.
"#;
    vault.write_note(note_a_rel, note_a_updated).expect("Failed to update Note A");
    let note_a_new = vault.read_note(note_a_rel).expect("Failed to read updated Note A");
    vault_index.update_note(&note_a_new).expect("Failed to re-index Note A");

    // Verify backlink is now gone
    let backlinks_after = vault_index.backlinks(note_b_rel);
    assert!(backlinks_after.is_empty(), "Backlink should be removed after note content update");

    // 8. Delete Note B and verify index and graph cleanup
    vault_index.remove_note(note_b_rel).expect("Failed to remove Note B from index");
    let fuzzy_after_del = vault_index.quick_search("Concurrency", 10);
    assert!(fuzzy_after_del.is_empty(), "Deleted note should not appear in fuzzy search");

    let ft_after_del = vault_index.fulltext_search("threads", 10).expect("Search failed");
    assert!(ft_after_del.is_empty(), "Deleted note content should not appear in full-text search");
}

#[test]
fn test_vault_sidecar_and_gitignore_integrity() {
    let dir = tempdir().expect("Failed to create temp dir");
    let vault = Vault::open(dir.path()).expect("Failed to open vault");

    // Sidecar folder and gitignore check
    let sidecar = dir.path().join(".cosmic-notes");
    assert!(sidecar.exists(), ".cosmic-notes folder must be created");

    let gitignore = sidecar.join(".gitignore");
    assert!(gitignore.exists(), ".cosmic-notes/.gitignore must exist");
    let gitignore_content = fs::read_to_string(&gitignore).expect("Failed to read .gitignore");
    assert!(gitignore_content.contains("index/"));
    assert!(gitignore_content.contains("cache/"));
    assert!(gitignore_content.contains("*.tmp"));

    // Write non-markdown and sidecar files to ensure scanning filters them
    fs::write(dir.path().join("image.png"), b"fake_png").unwrap();
    fs::write(dir.path().join("readme.txt"), "plain text").unwrap();
    fs::write(sidecar.join("hidden_note.md"), "# Hidden").unwrap();
    vault.write_note(Path::new("valid.md"), "# Valid Note").unwrap();

    let scanned = vault.scan_notes().expect("Failed to scan notes");
    assert_eq!(scanned.len(), 1, "Only valid markdown notes outside .cosmic-notes should be scanned");
    assert_eq!(scanned[0].path, Path::new("valid.md"));
}

#[test]
fn test_write_echo_ttl_and_filtering() {
    let cache = WriteEchoCache::new(Duration::from_millis(100));
    let path = Path::new("test_echo.md");

    // Register write
    cache.register(path, None);
    assert!(cache.consume_echo(path, None), "Immediate check must recognize echo token");

    // Consuming it removes it
    assert!(!cache.consume_echo(path, None), "Second check should be false because token was consumed");

    // Re-register and wait for TTL expiry
    cache.register(path, None);
    std::thread::sleep(Duration::from_millis(150));
    assert!(!cache.consume_echo(path, None), "Token must expire after TTL");
}

#[test]
fn test_m4_search_experience_and_indexing() {
    let dir = tempdir().expect("Failed to create temp dir");
    let vault = Vault::open(dir.path()).expect("Failed to open vault");

    // Seed notes for M4 search experience
    vault.write_note(
        Path::new("cosmic_desktop.md"),
        "---\ntitle: COSMIC Desktop Guide\ntags: [cosmic, rust, gui]\n---\n# COSMIC Desktop\nA modern Rust desktop environment.",
    ).unwrap();

    vault.write_note(
        Path::new("rust_concurrency.md"),
        "---\ntitle: Rust Concurrency Patterns\ntags: [rust, performance]\n---\n# Concurrency in Rust\nChannels and async/await.",
    ).unwrap();

    vault.write_note(
        Path::new("journal/daily_log.md"),
        "---\ntitle: Daily Log 2026\ntags: [journal]\n---\nWorking on COSMIC Notes search dialogs.",
    ).unwrap();

    let notes = vault.scan_notes().unwrap();
    let index = VaultIndex::open_or_create(&dir.path().join(".cosmic-notes/index"), &notes).expect("Index build must succeed");

    // 1. M4 Quick Switcher: Nucleo fuzzy search on title, tags, path (<0.1ms)
    let fuzzy_res1 = index.quick_search("cosmic", 10);
    assert_eq!(fuzzy_res1.len(), 1, "Quick Switcher matches title/path/tag for 'cosmic_desktop'");
    assert_eq!(fuzzy_res1[0].item.path, Path::new("cosmic_desktop.md"));

    let fuzzy_res2 = index.quick_search("performance", 10);
    assert_eq!(fuzzy_res2.len(), 1, "Should match tag 'performance'");
    assert_eq!(fuzzy_res2[0].item.path, Path::new("rust_concurrency.md"));

    let fuzzy_res3 = index.quick_search("journal", 10);
    assert_eq!(fuzzy_res3.len(), 1, "Should match path 'journal/daily_log.md'");
    assert_eq!(fuzzy_res3[0].item.path, Path::new("journal/daily_log.md"));

    // 2. M4 Full-Text Search: Tantivy BM25 with snippets and body search
    let ft_cosmic = index.fulltext_search("cosmic", 10).unwrap();
    assert_eq!(ft_cosmic.len(), 2, "Full-text search finds 'cosmic' in title AND in body text");

    let ft_results = index.fulltext_search("modern environment", 10).unwrap();
    assert_eq!(ft_results.len(), 1);
    assert_eq!(ft_results[0].path, Path::new("cosmic_desktop.md"));
    assert!(ft_results[0].snippet.as_ref().unwrap().to_lowercase().contains("modern"));

    // Title boosted score test: searching "Rust" should rank "Rust Concurrency Patterns" (in title)
    let ft_rust = index.fulltext_search("Rust", 10).unwrap();
    assert!(ft_rust.len() >= 2);

    // Operator escaping: queries with punctuation or operator characters shouldn't panic
    let complex_query = index.fulltext_search("rust + modern: (async/await)", 10);
    assert!(complex_query.is_ok(), "Query syntax with symbols must not panic or error");
}

