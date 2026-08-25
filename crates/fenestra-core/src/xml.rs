//! Namespace constants and escaping shared by every capabilities document.

pub const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";
pub const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";
pub const OWS_1_1_NAMESPACE: &str = "http://www.opengis.net/ows/1.1";
pub const OWS_2_0_NAMESPACE: &str = "http://www.opengis.net/ows/2.0";

/// Escape the markup characters in element text and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_replaces_markup_characters() {
        assert_eq!(escape(r#"a&b<c>"d'"#), "a&amp;b&lt;c&gt;&quot;d&apos;");
    }

    #[test]
    fn escape_leaves_plain_text_alone() {
        assert_eq!(escape("demo_parcels"), "demo_parcels");
    }
}
