pub mod fastembed;
pub mod openai;

/// Compute embedding input text from title and abstract.
pub fn make_input_text(title: &str, abstract_text: &str) -> String {
    format!("{}\n\n{}", title, abstract_text)
}

/// Compute the input hash for cache key.
pub fn compute_input_hash(
    provider_id: &str,
    model_id: &str,
    config_hash: &str,
    input_text: &str,
) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(provider_id.as_bytes());
    hasher.update(model_id.as_bytes());
    hasher.update(config_hash.as_bytes());
    hasher.update(input_text.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_input_text_format() {
        let text = make_input_text("My Title", "My abstract text.");
        assert_eq!(text, "My Title\n\nMy abstract text.");
    }

    #[test]
    fn compute_input_hash_deterministic() {
        let h1 = compute_input_hash("openai", "model", "cfg", "input");
        let h2 = compute_input_hash("openai", "model", "cfg", "input");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_input_hash_different_inputs() {
        let h1 = compute_input_hash("openai", "model", "cfg", "input1");
        let h2 = compute_input_hash("openai", "model", "cfg", "input2");
        assert_ne!(h1, h2);
    }
}
