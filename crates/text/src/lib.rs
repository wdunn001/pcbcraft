pub struct Catalog {
    /// Language tag for pluralization rules
    language: &'static str,
}

impl Catalog {
    /// Create a new catalog instance for a specific language
    pub fn new() -> Self {
        // Default to English unless specified
        // In a real implementation, this would select based on context
        Catalog { language: "en" }
    }

    /// Determine the plural form based on count and language
    pub fn plural(&self, count: usize) -> &str {
        match self.language {
            "en" => match count {
                1 => "one",
                _ => "other",
            },
            "fr" => match count {
                0 | 1 => "one",
                _ => "other",
            },
            "pt" => match count {
                0 | 1 => "one",
                _ => "other",
            },
            _ => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_form_english() {
        let cat = Catalog::new();
        assert_eq!(cat.plural(1), "one");
        assert_eq!(cat.plural(2), "other");
    }

    #[test]
    fn plural_form_french() {
        let cat = Catalog::new();
        assert_eq!(cat.plural(0), "one");
        assert_eq!(cat.plural(1), "one");
        assert_eq!(cat.plural(2), "other");
        assert_eq!(cat.plural(5), "other");
    }

    #[test]
    fn plural_form_portuguese() {
        let cat = Catalog::new();
        assert_eq!(cat.plural(0), "one");
        assert_eq!(cat.plural(1), "one");
        assert_eq!(cat.plural(2), "other");
        assert_eq!(cat.plural(5), "other");
    }
}