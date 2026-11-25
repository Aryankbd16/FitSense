use serde::{Deserialize, Serialize};

use super::{Error, Result};

#[derive(Serialize)]
pub struct Category {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
}

macro_rules! catalogue {
    ($(($id:literal, $group:literal, $label:literal)),+ $(,)?) => {
        &[ $(Category { id: $id, group: $group, label: $label }),+ ]
    };
}

pub const CATEGORIES: &[Category] = catalogue![
    ("t-shirts", "Upper wear", "T-shirts"),
    ("polo-t-shirts", "Upper wear", "Polo T-shirts"),
    ("casual-shirts", "Upper wear", "Casual shirts"),
    ("formal-shirts", "Upper wear", "Formal shirts"),
    ("oversized-shirts", "Upper wear", "Oversized shirts"),
    ("jeans", "Lower wear", "Jeans"),
    ("formal-trousers", "Lower wear", "Formal trousers"),
    ("chinos", "Lower wear", "Chinos"),
    ("casual-trousers", "Lower wear", "Casual trousers"),
    ("cargo-pants", "Lower wear", "Cargo pants"),
    ("shorts", "Lower wear", "Shorts"),
    ("blazers", "Outerwear", "Blazers"),
    ("jackets", "Outerwear", "Jackets"),
    ("sweaters", "Outerwear", "Sweaters"),
    ("hoodies", "Outerwear", "Hoodies"),
    ("belts", "Accessories", "Belts"),
    ("watches", "Accessories", "Watches"),
    ("ties", "Accessories", "Ties"),
];

#[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GarmentInput {
    pub name: String,
    pub category: String,
    pub primary_colour: String,
    pub secondary_colours: Vec<String>,
    pub pattern: String,
    pub fit: String,
    pub sleeve: String,
    pub fabric: String,
    pub formality: String,
    pub seasons: Vec<String>,
    pub notes: String,
}

impl GarmentInput {
    pub fn validated(mut self) -> Result<Self> {
        for field in [
            &mut self.name,
            &mut self.primary_colour,
            &mut self.pattern,
            &mut self.fit,
            &mut self.sleeve,
            &mut self.fabric,
            &mut self.formality,
        ] {
            *field = field.trim().to_owned();
            if field.len() > 120 || field.chars().any(char::is_control) {
                return Err(Error::InvalidInput);
            }
        }
        self.notes = self.notes.trim().to_owned();
        if self.name.is_empty()
            || self.notes.len() > 4000
            || self
                .notes
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            || !CATEGORIES
                .iter()
                .any(|category| category.id == self.category)
            || self.secondary_colours.len() > 8
            || self.seasons.len() > 4
        {
            return Err(Error::InvalidInput);
        }
        for colour in &mut self.secondary_colours {
            *colour = colour.trim().to_owned();
            if colour.is_empty() || colour.len() > 60 || colour.chars().any(char::is_control) {
                return Err(Error::InvalidInput);
            }
        }
        if self
            .seasons
            .iter()
            .any(|season| !matches!(season.as_str(), "Spring" | "Summer" | "Autumn" | "Winter"))
        {
            return Err(Error::InvalidInput);
        }
        self.seasons.sort();
        self.seasons.dedup();
        Ok(self)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Garment {
    pub id: String,
    pub added: i64,
    pub changed: i64,
    pub photo: bool,
    pub details: GarmentInput,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "base64", rename_all = "snake_case")]
pub enum PhotoChange {
    Keep,
    Remove,
    Upload(String),
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    pub search: String,
    pub category: String,
    pub group: String,
    pub sort: Sort,
    pub offset: u32,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Newest,
    Oldest,
    Name,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub garments: Vec<Garment>,
    pub has_more: bool,
}
