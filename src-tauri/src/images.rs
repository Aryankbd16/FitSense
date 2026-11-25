#[derive(Clone, Copy)]
pub enum Variant {
    Original,
    Display,
    Thumbnail,
    AiInput,
}

impl Variant {
    pub const ALL: [Self; 4] = [
        Self::Original,
        Self::Display,
        Self::Thumbnail,
        Self::AiInput,
    ];

    pub fn directory(self) -> &'static str {
        match self {
            Self::Original => "images/originals",
            Self::Display => "images/display",
            Self::Thumbnail => "images/thumbnails",
            Self::AiInput => "images/ai-input",
        }
    }
}

/// Relative reference for an opaque 128-bit asset ID. A future encrypted image store owns writes.
pub fn reference(variant: Variant, id: &str) -> Option<String> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(format!("{}/{id}", variant.directory()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_references_use_only_relative_paths_and_opaque_ids() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            reference(Variant::Original, id).unwrap(),
            format!("images/originals/{id}")
        );
        for invalid in [
            "../portrait.jpg",
            "C:\\portrait.jpg",
            "portrait.jpg",
            "0123456789abcdef0123456789abcde/",
        ] {
            assert!(reference(Variant::Original, invalid).is_none());
        }
    }
}
