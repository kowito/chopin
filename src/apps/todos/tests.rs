#[cfg(test)]
mod tests {
    use super::services;

    #[tokio::test]
    async fn test_todos_not_found() {
        let result = services::get_by_id(999).await;
        assert!(result.is_err());
    }
}
