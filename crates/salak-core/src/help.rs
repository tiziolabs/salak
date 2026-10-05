//! Help pages, embedded in the binary.

pub struct Page {
    pub name: &'static str,
    pub title: &'static str,
    pub markdown: &'static str,
}

pub const PAGES: &[Page] = &[
    Page {
        name: "user-guide",
        title: "User Guide",
        markdown: include_str!("../help/user-guide.md"),
    },
    Page {
        name: "theming",
        title: "Theming Guide",
        markdown: include_str!("../help/theming.md"),
    },
];

pub fn page(name: &str) -> Option<&'static Page> {
    PAGES.iter().find(|page| page.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_pages_by_name() {
        assert_eq!(
            page("theming").map(|page| page.title),
            Some("Theming Guide")
        );
        assert!(page("nope").is_none());
        assert!(PAGES.iter().all(|page| !page.markdown.is_empty()));
    }
}
