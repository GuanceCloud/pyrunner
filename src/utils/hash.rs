use sha2::{Digest, Sha256};

pub fn sha256_string(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::sha256_string;

    #[test]
    fn sha256_is_stable() {
        let left = sha256_string("hello");
        let right = sha256_string("hello");
        assert_eq!(left, right);
        assert_eq!(left.len(), 64);
    }

    #[test]
    fn sha256_changes_with_content() {
        assert_ne!(sha256_string("hello"), sha256_string("world"));
    }
}
