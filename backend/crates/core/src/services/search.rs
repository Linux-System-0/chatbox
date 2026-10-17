//! Message search with membership enforcement.

use super::{AuthContext, Services};
use crate::error::{Error, Result};
use crate::model::{Message, SearchQuery, SortOrder};

/// Convert a shell-style wildcard (`*`, `?`) into an anchored regex.
pub fn wildcard_to_regex(pattern: &str) -> Result<regex::Regex> {
    if pattern.is_empty() {
        return Err(Error::Invalid("filename pattern must not be empty".into()));
    }
    let mut out = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            c => out.push_str(&regex::escape(&c.to_string())),
        }
    }
    out.push('$');
    regex::Regex::new(&out).map_err(|e| Error::Invalid(format!("invalid pattern: {e}")))
}

impl Services {
    /// Search messages the caller is allowed to see.
    pub async fn search(&self, ctx: &AuthContext, query: &SearchQuery) -> Result<Vec<Message>> {
        let mut query = query.clone();
        query.viewer_id = ctx.user_id;
        query.limit = query.limit.clamp(1, self.config.max_page_size);
        if query
            .content
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            query.content = None;
        }
        if let Some(filename) = &query.filename {
            wildcard_to_regex(filename)?;
        }
        if query.content.is_none()
            && query.filename.is_none()
            && query.from.is_none()
            && query.to.is_none()
        {
            return Err(Error::Invalid(
                "at least one of content, filename, from, or to is required".into(),
            ));
        }
        if let Some(conversation_id) = query.conversation_id {
            self.require_member(conversation_id, ctx.user_id).await?;
        }
        if let (Some(from), Some(to)) = (query.from, query.to) {
            if from > to {
                return Err(Error::Invalid("from must not be after to".into()));
            }
        }
        if query.sort != SortOrder::Asc && query.sort != SortOrder::Desc {
            return Err(Error::Invalid("invalid sort order".into()));
        }
        self.stores.messages.search(&query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_compile() {
        let re = wildcard_to_regex("*.mp4").unwrap();
        assert!(re.is_match("clip.mp4"));
        assert!(!re.is_match("clip.mp3"));
        let re = wildcard_to_regex("report?.pdf").unwrap();
        assert!(re.is_match("report1.pdf"));
        assert!(!re.is_match("report12.pdf"));
    }

    #[test]
    fn dots_are_literal() {
        let re = wildcard_to_regex("a.b").unwrap();
        assert!(re.is_match("a.b"));
        assert!(!re.is_match("axb"));
    }
}
