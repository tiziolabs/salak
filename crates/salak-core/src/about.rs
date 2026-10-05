//! Shown by Help › About Salak, from the package metadata.

pub struct About {
    pub version: &'static str,
    pub license: &'static str,
    pub author: String,
    pub repository: &'static str,
}

pub fn about() -> About {
    // `Name <email>`: only the name is shown.
    let authors = env!("CARGO_PKG_AUTHORS");
    let author = authors.split(':').next().unwrap_or_default();
    About {
        version: env!("CARGO_PKG_VERSION"),
        license: env!("CARGO_PKG_LICENSE"),
        author: author.split(" <").next().unwrap_or_default().to_string(),
        repository: env!("CARGO_PKG_REPOSITORY"),
    }
}
