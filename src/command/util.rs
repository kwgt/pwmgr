/*
 * Password manager
 *
 *  Copyright (C) 2025 Hiroshi KUWAGATA
 */

///
/// 文字列が空文字、または空白文字のみで構成されているかを判定する
///
pub(crate) fn is_blank(s: &str) -> bool {
    s.is_empty() || s.chars().all(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::is_blank;

    ///
    /// 空文字/空白のみがtrue、それ以外はfalseになることを確認
    ///
    #[test]
    fn blank_empty_and_spaces() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("　　")); // 全角空白
        assert!(!is_blank("a"));
        assert!(!is_blank(" a "));
    }
}
