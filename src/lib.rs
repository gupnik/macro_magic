pub use macros::export_tokens;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_tokens() {
        #[export_tokens]
        fn add_stuff(a: usize, b: usize) -> usize {
            a + b
        }
        assert_eq!(
            __EXPORT_TOKENS__ADD_STUFF,
            "fn add_stuff(a : usize, b : usize) -> usize { a + b }"
        );
    }
}
