//! Persisted presentation only. Widths are whole CSS pixels, not media units.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ColumnId {
    Issue,
    Number,
    Start,
    End,
    Duration,
    Cps,
    Rs,
    LongestLine,
    LineCount,
    Gap,
    Style,
    Actor,
    Text,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    Comfortable,
    Compact,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    pub id: ColumnId,
    pub visible: bool,
    pub width: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Columns {
    pub items: Vec<Column>,
    pub density: Density,
}
impl Default for Columns {
    fn default() -> Self {
        use ColumnId::*;
        Self {
            density: Density::Compact,
            items: [
                (Issue, true, 24),
                (Number, true, 44),
                (Start, true, 104),
                (End, true, 104),
                (Duration, false, 80),
                (Cps, false, 76),
                (Rs, false, 64),
                (LongestLine, false, 100),
                (LineCount, false, 80),
                (Gap, false, 90),
                (Style, false, 120),
                (Actor, false, 120),
                (Text, true, 240),
            ]
            .into_iter()
            .map(|(id, visible, width)| Column { id, visible, width })
            .collect(),
        }
    }
}
impl Columns {
    pub fn validate(&self) -> Result<(), String> {
        if self.items.len() != 13 || self.items.first().map(|c| c.id) != Some(ColumnId::Issue) {
            return Err("Columns must contain every supported column, with Issues first".into());
        }
        let mut seen = HashSet::new();
        for column in &self.items {
            if !seen.insert(column.id) {
                return Err("Column order contains a duplicate".into());
            }
            if matches!(column.id, ColumnId::Issue | ColumnId::Text) && !column.visible {
                return Err("Issues and Text columns must remain visible".into());
            }
            let valid_width = match column.id {
                ColumnId::Issue => column.width == 24,
                ColumnId::Text => (120..=1600).contains(&column.width),
                _ => (40..=600).contains(&column.width),
            };
            if !valid_width {
                return Err("Column width is outside its supported range".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_visibility_and_widths_preserve_mandatory_columns() {
        let mut columns = Columns::default();
        columns.items.swap(1, 4);
        columns.items[1].visible = true;
        columns.items[1].width = 150;
        columns.validate().unwrap();
        let encoded = serde_json::to_vec(&columns).unwrap();
        assert_eq!(
            serde_json::from_slice::<Columns>(&encoded).unwrap(),
            columns
        );
        let mut invalid = columns.clone();
        invalid.items[0].visible = false;
        assert!(invalid.validate().is_err());
        invalid = columns.clone();
        invalid.items[12].visible = false;
        assert!(invalid.validate().is_err());
        invalid = columns.clone();
        invalid.items[2].id = invalid.items[1].id;
        assert!(invalid.validate().is_err());
        invalid = columns;
        invalid.items[1].width = 0;
        assert!(invalid.validate().is_err());
    }
}
