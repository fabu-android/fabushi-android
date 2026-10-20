pub const SAND_PRODUCT_DISPLAY_NAME: &str = "Fabushi";
pub const SAND_PRODUCT_HTTP_TOKEN: &str = "Fabushi";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_token_has_no_whitespace() {
        assert!(!SAND_PRODUCT_HTTP_TOKEN.chars().any(char::is_whitespace));
        assert_eq!(SAND_PRODUCT_DISPLAY_NAME.replace(char::is_whitespace, ""), SAND_PRODUCT_HTTP_TOKEN);
    }
}
