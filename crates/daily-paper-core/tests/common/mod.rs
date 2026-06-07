use daily_paper_core::config::{
    ResolvedConfig, ResolvedEmailConfig, ResolvedEmbeddingConfig, ResolvedPdfConfig,
    ResolvedReaderConfig, ResolvedRerankerConfig, ResolvedScheduleConfig, ResolvedSourceConfig,
    ResolvedWebConfig, ResolvedZoteroConfig,
};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Start a mock HTTP server and return it.
pub async fn start_mock_server() -> MockServer {
    MockServer::start().await
}

/// Set up Zotero API mocks for items and collections.
#[allow(dead_code)]
pub async fn setup_zotero_mock(server: &MockServer, user_id: &str) {
    let items_json = include_str!("../fixtures/zotero_items.json");
    let collections_json = include_str!("../fixtures/zotero_collections.json");

    // Mock items endpoint - returns version header and items
    Mock::given(method("GET"))
        .and(path(format!("/users/{}/items", user_id)))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("Last-Modified-Version", "42")
                .set_body_string(items_json),
        )
        .mount(server)
        .await;

    // Mock collections fetch
    Mock::given(method("GET"))
        .and(path(format!("/users/{}/collections", user_id)))
        .respond_with(ResponseTemplate::new(200).set_body_string(collections_json))
        .mount(server)
        .await;
}

/// Set up arXiv API mock.
#[allow(dead_code)]
pub async fn setup_arxiv_mock(server: &MockServer) {
    let feed_xml = include_str!("../fixtures/arxiv_rss.xml");

    Mock::given(method("GET"))
        .and(path("/rss/cs.AI"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed_xml))
        .mount(server)
        .await;
}

/// Set up arXiv export API mock.
#[allow(dead_code)]
pub async fn setup_arxiv_export_mock(server: &MockServer) {
    let feed_xml = include_str!("../fixtures/arxiv_export_feed.xml");

    Mock::given(method("GET"))
        .and(path("/api/query"))
        .and(query_param("start", "0"))
        .and(query_param("max_results", "1000"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed_xml))
        .mount(server)
        .await;
}

/// Set up OpenAI embedding API mock with deterministic vectors.
#[allow(dead_code)]
pub async fn setup_embedding_mock(server: &MockServer, dimensions: usize) {
    let embedding: Vec<f32> = (0..dimensions)
        .map(|i| (i as f32) / (dimensions as f32))
        .collect();

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [
                {"embedding": embedding}
            ]
        })))
        .mount(server)
        .await;
}

/// Set up OpenAI embedding API mock that returns multiple embeddings.
#[allow(dead_code)]
pub async fn setup_embedding_batch_mock(server: &MockServer, dimensions: usize, batch_size: usize) {
    let embedding: Vec<f32> = (0..dimensions)
        .map(|i| (i as f32) / (dimensions as f32))
        .collect();
    let data: Vec<serde_json::Value> = (0..batch_size)
        .map(|_| json!({"embedding": embedding}))
        .collect();

    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": data})))
        .mount(server)
        .await;
}

/// Set up OpenAI chat completions API mock.
pub async fn setup_reader_mock(server: &MockServer, summary: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [
                {
                    "message": {
                        "content": summary
                    }
                }
            ],
            "usage": {
                "prompt_tokens": 1500,
                "completion_tokens": 200,
                "total_tokens": 1700
            }
        })))
        .mount(server)
        .await;
}

/// Set up PDF download mock with minimal PDF bytes.
#[allow(dead_code)]
pub async fn setup_pdf_mock(server: &MockServer) {
    let pdf_bytes = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

    Mock::given(method("GET"))
        .and(path("/pdf"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(pdf_bytes.to_vec())
                .insert_header("content-type", "application/pdf"),
        )
        .expect(1..)
        .mount(server)
        .await;
}

/// Make a test ResolvedConfig pointing at the mock server.
#[allow(dead_code)]
pub fn make_test_config(mock_server_url: &str) -> ResolvedConfig {
    ResolvedConfig {
        zotero: ResolvedZoteroConfig {
            user_id: "test_user".to_string(),
            api_key: "test_key".to_string(),
            max_snapshot_age_hours: 168,
            filters: vec![],
        },
        sources: vec![ResolvedSourceConfig {
            kind: "arxiv".to_string(),
            backend: "rss".to_string(),
            categories: vec!["cs.AI".to_string()],
            include_cross_list: false,
            max_results_per_page: 1000,
            max_pages: 3,
        }],
        embedding: ResolvedEmbeddingConfig {
            kind: "openai-compatible".to_string(),
            base_url: Some(format!("{}/v1", mock_server_url)),
            api_key: Some("test_key".to_string()),
            model: "test-model".to_string(),
            batch_size: 64,
            timeout_secs: 10,
            max_retries: 2,
            max_concurrency: 2,
        },
        reranker: ResolvedRerankerConfig {
            kind: "embedding_similarity".to_string(),
            top_k_library_matches: 10,
        },
        reader: ResolvedReaderConfig {
            kind: "openai-compatible".to_string(),
            base_url: format!("{}/v1", mock_server_url),
            api_key: "test_key".to_string(),
            model: "test-model".to_string(),
            top_n: 3,
            language: "zh-CN".to_string(),
            require_full_text: false,
            on_read_failure: "block".to_string(),
            timeout_secs: 10,
            max_retries: 2,
            max_concurrency: 2,
            max_input_tokens: 4000,
        },
        pdf: ResolvedPdfConfig {
            extractor: "pdftotext".to_string(),
            timeout_secs: 10,
            max_pdf_mb: 10,
            max_text_chars: 100000,
        },
        email: ResolvedEmailConfig {
            smtp_server: "localhost".to_string(),
            smtp_port: 25,
            sender: "test@example.com".to_string(),
            receiver: "test@example.com".to_string(),
            password: "test".to_string(),
        },
        web: ResolvedWebConfig {
            port: 8991,
            ui_path: ".daily-paper/ui".to_string(),
        },
        schedule: ResolvedScheduleConfig {
            enabled: false,
            hour: 7,
            minute: 30,
        },
    }
}
