//! Bounded presentation schema shared by user preferences and project snapshots.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Panel {
    Waveform,
    Video,
    Transport,
    Cues,
    Editor,
    Review,
    Translation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Row {
    pub panels: Vec<Panel>,
    pub weight: u8,
    pub split: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Arrangement {
    pub version: u8,
    pub rows: Vec<Row>,
}

impl Arrangement {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.rows.is_empty() || self.rows.len() > 7 {
            return Err("Unsupported or empty panel arrangement".into());
        }
        let mut seen = Vec::new();
        for row in &self.rows {
            if !(1..=2).contains(&row.panels.len())
                || !(1..=100).contains(&row.weight)
                || !(20..=80).contains(&row.split)
            {
                return Err("Invalid panel row or proportions".into());
            }
            for panel in &row.panels {
                if seen.contains(panel) {
                    return Err("A panel may appear only once".into());
                }
                seen.push(*panel);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrangement_round_trip_and_invalid_shapes() {
        let value = Arrangement {
            version: 1,
            rows: vec![Row {
                panels: vec![Panel::Waveform, Panel::Video],
                weight: 13,
                split: 65,
            }],
        };
        value.validate().unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<Arrangement>(&bytes).unwrap(),
            value
        );
        for candidate in [
            Arrangement {
                version: 2,
                ..value.clone()
            },
            Arrangement {
                rows: vec![],
                ..value.clone()
            },
            Arrangement {
                rows: vec![value.rows[0].clone(), value.rows[0].clone()],
                ..value.clone()
            },
            Arrangement {
                rows: vec![Row {
                    panels: vec![Panel::Cues],
                    weight: 0,
                    split: 50,
                }],
                ..value.clone()
            },
            Arrangement {
                rows: vec![Row {
                    panels: vec![Panel::Cues],
                    weight: 10,
                    split: 81,
                }],
                ..value.clone()
            },
        ] {
            assert!(candidate.validate().is_err());
        }
    }
}
