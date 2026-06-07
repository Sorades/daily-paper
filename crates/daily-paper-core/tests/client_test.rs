mod common;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

/// Test Zotero client fetches items correctly.
#[tokio::test]
async fn test_zotero_fetch_items() {
    let server = common::start_mock_server().await;
    common::setup_zotero_mock(&server, "test_user").await;

    let client = daily_paper_core::zotero::client::ZoteroClient::new(
        "test_user".to_string(),
        "test_key".to_string(),
    )
    .with_base_url(&server.uri());

    let version = client.get_library_version().await.unwrap();
    assert_eq!(version, Some(42));

    let items = client.fetch_items(None).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["data"]["title"], "Attention Is All You Need");
}

/// Test Zotero client fetches collections correctly.
#[tokio::test]
async fn test_zotero_fetch_collections() {
    let server = common::start_mock_server().await;
    common::setup_zotero_mock(&server, "test_user").await;

    let client = daily_paper_core::zotero::client::ZoteroClient::new(
        "test_user".to_string(),
        "test_key".to_string(),
    )
    .with_base_url(&server.uri());

    let collections = client.fetch_collections().await.unwrap();
    assert_eq!(collections.len(), 2);
    assert_eq!(collections[0]["data"]["name"], "ML Papers");
}

/// Test Zotero client handles 429 with retry (via fetch_items which uses get_with_retry).
#[tokio::test]
async fn test_zotero_429_retry() {
    let server = common::start_mock_server().await;

    // First request returns 429, second succeeds
    Mock::given(method("GET"))
        .and(path("/users/test_user/items"))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "1"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/users/test_user/items"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("Last-Modified-Version", "42")
                .set_body_string("[]"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = daily_paper_core::zotero::client::ZoteroClient::new(
        "test_user".to_string(),
        "test_key".to_string(),
    )
    .with_base_url(&server.uri());

    // Use fetch_items (which retries) instead of get_library_version (which doesn't)
    let items = client.fetch_items(None).await.unwrap();
    assert!(items.is_empty());
}

/// Test arXiv client fetches papers correctly.
#[tokio::test]
async fn test_arxiv_fetch_papers() {
    let server = common::start_mock_server().await;
    common::setup_arxiv_mock(&server).await;

    let client =
        daily_paper_core::source::arxiv::client::ArxivClient::new(vec!["cs.AI".to_string()], false)
            .with_base_url(&format!("{}/rss", server.uri()));

    let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let papers = client.fetch(start, end).await.unwrap();
    assert_eq!(papers.len(), 2);
    // Papers are sorted by paper_id, so check both exist
    let titles: Vec<&str> = papers.iter().map(|p| p.title.as_str()).collect();
    assert!(titles.iter().any(|t| t.contains("Attention")));
    assert!(titles.iter().any(|t| t.contains("Efficient")));
    assert!(papers
        .iter()
        .all(|p| p.source_metadata["announce_type"] == "new"));
}

/// Test arXiv export backend fetches papers by submittedDate query.
#[tokio::test]
async fn test_arxiv_export_fetch_papers() {
    let server = common::start_mock_server().await;
    common::setup_arxiv_export_mock(&server).await;

    let client = daily_paper_core::source::arxiv::client::ArxivClient::with_backend(
        daily_paper_core::source::arxiv::client::ArxivBackendKind::Export,
        vec!["cs.AI".to_string(), "cs.LG".to_string()],
        true,
        1000,
        3,
    )
    .with_export_base_url(&format!("{}/api/query", server.uri()));

    let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let papers = client.fetch(start, end).await.unwrap();

    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].title, "Export API Paper");
    assert_eq!(papers[0].arxiv_id, Some("2301.12345".into()));
    assert_eq!(papers[0].doi, Some("10.48550/arxiv.2301.12345".into()));
}

/// Test arXiv export backend fails instead of silently truncating over page limit.
#[tokio::test]
async fn test_arxiv_export_page_limit_fails() {
    let server = common::start_mock_server().await;
    let feed_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/">
  <opensearch:totalResults>2</opensearch:totalResults>
  <entry>
    <id>http://arxiv.org/abs/2301.12345v1</id>
    <title>First Page Paper</title>
    <summary>A test abstract.</summary>
    <author><name>Alice Smith</name></author>
    <published>2023-01-30T00:00:00Z</published>
    <updated>2023-01-31T00:00:00Z</updated>
    <category term="cs.AI"/>
    <link href="http://arxiv.org/abs/2301.12345v1" rel="alternate"/>
  </entry>
</feed>"#;

    Mock::given(method("GET"))
        .and(path("/api/query"))
        .and(query_param("start", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed_xml))
        .mount(&server)
        .await;

    let client = daily_paper_core::source::arxiv::client::ArxivClient::with_backend(
        daily_paper_core::source::arxiv::client::ArxivBackendKind::Export,
        vec!["cs.AI".to_string()],
        true,
        1,
        1,
    )
    .with_export_base_url(&format!("{}/api/query", server.uri()));

    let start = chrono::NaiveDate::from_ymd_opt(2023, 1, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let end = chrono::NaiveDate::from_ymd_opt(2023, 1, 31)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let err = client.fetch(start, end).await.unwrap_err();
    assert!(err.to_string().contains("page limit"));
}

/// Test embedding client works correctly.
#[tokio::test]
async fn test_embedding_batch() {
    let server = common::start_mock_server().await;
    common::setup_embedding_batch_mock(&server, 5, 2).await;

    let client = daily_paper_core::embedding::openai::EmbeddingClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        64,
        10,
        2,
        2,
    );

    let texts = vec!["hello world".to_string(), "test text".to_string()];
    let embeddings = client.embed_batch(&texts).await.unwrap();

    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0].len(), 5);
    assert_eq!(embeddings[0][0], 0.0);
    assert_eq!(embeddings[0][4], 0.8);
}

/// Test embedding client handles 429 with retry.
#[tokio::test]
async fn test_embedding_429_retry() {
    let server = common::start_mock_server().await;

    // First request returns 429, second succeeds
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "1"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    let embedding: Vec<f32> = (0..5).map(|i| i as f32 * 0.1).collect();
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"embedding": embedding}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = daily_paper_core::embedding::openai::EmbeddingClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        64,
        10,
        2,
        2,
    );

    let embeddings = client.embed_batch(&["test".to_string()]).await.unwrap();
    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].len(), 5);
}

/// Test reader client works correctly.
#[tokio::test]
async fn test_reader_complete() {
    let server = common::start_mock_server().await;
    common::setup_reader_mock(&server, "这是一篇关于注意力机制的论文摘要。").await;

    let client = daily_paper_core::reader::openai::ReaderClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        10,
        2,
        2,
        4000,
    );

    let (summary, usage) = client
        .complete("system prompt", "user prompt")
        .await
        .unwrap();
    assert!(summary.contains("注意力机制"));
    assert!(usage.is_some());
    let usage = usage.unwrap();
    assert_eq!(usage.input_tokens, Some(1500));
    assert_eq!(usage.output_tokens, Some(200));
}

/// Test reader client returns error on 5xx (not retried, LLM errors are not retryable).
#[tokio::test]
async fn test_reader_5xx_fails() {
    let server = common::start_mock_server().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;

    let client = daily_paper_core::reader::openai::ReaderClient::new(
        format!("{}/v1", server.uri()),
        "test_key".to_string(),
        "test-model".to_string(),
        10,
        2,
        2,
        4000,
    );

    let result = client.complete("system", "user").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("500"));
}
