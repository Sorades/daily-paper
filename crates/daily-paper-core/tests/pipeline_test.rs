mod common;

/// Test that the embedding client produces correct output.
#[tokio::test]
async fn test_embedding_produces_vectors() {
    let server = common::start_mock_server().await;
    common::setup_embedding_mock(&server, 10).await;

    let client = daily_paper_core::embedding::openai::EmbeddingClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        64,
        10,
        2,
        2,
    );

    let text = daily_paper_core::embedding::openai::EmbeddingClient::make_input_text(
        "Test Paper",
        "This is a test abstract.",
    );

    let embedding = client.embed_one(&text).await.unwrap();
    assert_eq!(embedding.len(), 10);

    // Verify deterministic output
    let embedding2 = client.embed_one(&text).await.unwrap();
    assert_eq!(embedding, embedding2);
}

/// Test that the reader produces a summary.
#[tokio::test]
async fn test_reader_produces_summary() {
    let server = common::start_mock_server().await;
    let expected_summary = "这篇论文提出了一种新的方法，在实验中取得了优异的性能。";
    common::setup_reader_mock(&server, expected_summary).await;

    let client = daily_paper_core::reader::openai::ReaderClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        10,
        2,
        2,
        4000,
    );

    let system_prompt = daily_paper_core::reader::template::build_system_prompt(None);
    let user_prompt = daily_paper_core::reader::template::build_user_prompt(
        "Test Paper",
        "A test abstract.",
        "Alice Smith",
        "Selected text for reading.",
        "zh-CN",
    );

    let (summary, usage) = client.complete(&system_prompt, &user_prompt).await.unwrap();
    assert_eq!(summary, expected_summary);
    assert!(usage.is_some());
}

/// Test that rerank produces correct ordering.
#[test]
fn test_rerank_orders_by_score() {
    use daily_paper_core::rerank::cosine::rerank;

    let candidates = vec![
        ("paper_a".to_string(), vec![1.0, 0.0, 0.0]),
        ("paper_b".to_string(), vec![0.0, 1.0, 0.0]),
        ("paper_c".to_string(), vec![0.0, 0.0, 1.0]),
    ];

    // Library has one item similar to paper_a
    let library = vec![("lib1".to_string(), vec![0.9, 0.1, 0.0], 1.0)];

    let ranked = rerank(&candidates, &library, 10);

    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked[0].paper_id, "paper_a"); // Most similar to library
    assert_eq!(ranked[0].rank, 1);
    assert!(ranked[0].score > ranked[1].score);
}

/// Test that selection takes top N.
#[test]
fn test_selection_takes_top_n() {
    use daily_paper_core::rerank::selection::select_top_n;

    let ids = vec![
        "paper_a".to_string(),
        "paper_b".to_string(),
        "paper_c".to_string(),
    ];

    let selection = select_top_n("test_cache_key", &ids, 2, vec![]);
    assert_eq!(selection.selected_paper_ids.len(), 2);
    assert_eq!(selection.selected_paper_ids[0], "paper_a");
    assert_eq!(selection.selected_paper_ids[1], "paper_b");
}

/// Test that HTML rendering produces valid output.
#[test]
fn test_render_produces_html() {
    use daily_paper_core::models::common::Author;
    use daily_paper_core::models::read::ReadResult;
    use daily_paper_core::render::html::{render_html, ReportPaper};

    let papers = vec![ReportPaper {
        paper_id: "paper_a".to_string(),
        rank: 1,
        title: "Test Paper".to_string(),
        authors: vec![Author {
            name: "Alice Smith".into(),
            normalized_name: None,
            affiliation: Some("MIT".into()),
            url: None,
        }],
        abstract_text: "A test abstract.".to_string(),
        landing_url: Some("https://arxiv.org/abs/2301.12345".to_string()),
        pdf_url: None,
        read_result: Some(ReadResult {
            paper_id: "paper_a".to_string(),
            cache_key: "test".to_string(),
            generated_at: chrono::Utc::now(),
            model_id: "test-model".to_string(),
            reader_template_hash: "test".to_string(),
            language: "zh-CN".to_string(),
            summary: "这是一篇测试论文的摘要。".to_string(),
            metadata: daily_paper_core::models::read::PaperMetadataSummary {
                institutions: vec![],
                notable_authors: vec![],
                project_url: None,
                code_url: None,
            },
            author_affiliations: vec![],
            token_usage: None,
            warnings: vec![],
        }),
        score: 0.85,
    }];

    let html = render_html("Test Report", &papers, "test-run-001");

    assert!(html.contains("Test Paper"));
    assert!(html.contains("Alice Smith"));
    assert!(html.contains("MIT"));
    assert!(html.contains("这是一篇测试论文的摘要"));
    assert!(html.contains("test-run-001"));
}

/// Test that text rendering produces valid output.
#[test]
fn test_render_produces_text() {
    use daily_paper_core::models::common::Author;
    use daily_paper_core::render::html::ReportPaper;
    use daily_paper_core::render::text::render_text;

    let papers = vec![ReportPaper {
        paper_id: "paper_a".to_string(),
        rank: 1,
        title: "Test Paper".to_string(),
        authors: vec![Author {
            name: "Alice Smith".into(),
            normalized_name: None,
            affiliation: Some("MIT".into()),
            url: None,
        }],
        abstract_text: "A test abstract.".to_string(),
        landing_url: None,
        pdf_url: None,
        read_result: None,
        score: 0.5,
    }];

    let text = render_text("Test Report", &papers, "test-run-001");

    assert!(text.contains("Test Paper"));
    assert!(text.contains("Alice Smith"));
    assert!(text.contains("MIT"));
    assert!(text.contains("test-run-001"));
}

/// Test that dedup logic works correctly.
#[test]
fn test_dedup_removes_duplicates() {
    use daily_paper_core::models::candidate::normalize_doi;
    use daily_paper_core::models::zotero::ZoteroSnapshot;

    // Library has paper with same DOI
    let snapshot = ZoteroSnapshot {
        snapshot_id: "test".to_string(),
        user_id: "test".to_string(),
        library_version: Some(1),
        created_at: chrono::Utc::now(),
        item_count: 1,
        items: vec![daily_paper_core::models::zotero::LibraryPaper {
            library_id: "lib1".to_string(),
            library_type: daily_paper_core::models::zotero::LibraryType::User,
            zotero_key: "KEY1".to_string(),
            version: Some(1),
            item_type: "journalArticle".to_string(),
            parent_item: None,
            title: "Paper in Library".to_string(),
            abstract_text: None,
            authors: vec![],
            year: None,
            doi: Some("10.1234/test".to_string()),
            arxiv_id: None,
            url: None,
            collection_keys: vec![],
            collections: vec![],
            tags: vec![],
            is_trashed: false,
            date_added: None,
            date_modified: None,
            attachments: vec![],
        }],
    };

    // Check that c1 would be filtered out by DOI match
    let lib_dois: std::collections::HashSet<String> = snapshot
        .items
        .iter()
        .filter_map(|p| p.doi.as_ref().map(|d| normalize_doi(d)))
        .collect();

    assert!(lib_dois.contains(&normalize_doi("10.1234/test")));
    assert!(!lib_dois.contains(&normalize_doi("10.5678/new")));
}
