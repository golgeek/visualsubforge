use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EffectSegment {
    pub text: String,
    pub warning: bool,
}

impl EffectSegment {
    pub(super) fn plain(text: String) -> Self {
        Self {
            text,
            warning: false,
        }
    }
}

pub(super) fn spacing(before: i64, after: i64) -> EffectSegment {
    let text = match (before < 0, after < 0) {
        (true, true) => format!("overlap {} → {} ms", -before, -after),
        (true, false) => format!("overlap {} ms → gap {after} ms", -before),
        _ => format!("gap {before} → {after} ms"),
    };
    EffectSegment::plain(text)
}

pub(super) fn speed(effect: &Effect, first: u32) -> Option<EffectSegment> {
    if effect.index != first
        && matches!((effect.before,effect.after),(Some(a),Some(b)) if (a-b).abs()<0.1)
    {
        return None;
    }
    let prefix = if effect.index == first {
        String::new()
    } else {
        format!("cue {} ", effect.index + 1)
    };
    let label = if effect.rule == Rule::CpsHigh {
        "CPS"
    } else {
        "RS"
    };
    let show = |value: Option<f64>| value.map_or_else(|| "—".into(), |value| format!("{value:.1}"));
    let violates = effect.after.is_some_and(|value| {
        value > f64::from(effect.limit)
            || effect.rule == Rule::RsHigh && value == f64::from(effect.limit)
    });
    let mut text = format!(
        "{prefix}{label} {} → {}",
        show(effect.before),
        show(effect.after)
    );
    if violates {
        text.push_str(&format!(" (limit {})", effect.limit));
    }
    Some(EffectSegment {
        text,
        warning: violates && effect.after > effect.before,
    })
}

/// Identify the cut responsible for the original finding, including asymmetric buffers.
pub(super) fn shot(
    label: &str,
    before: u64,
    after: u64,
    scenes: &Scenes,
    profile: &super::super::Profile,
) -> Option<(EffectSegment, String)> {
    let cuts = match scenes {
        Scenes::Complete(cuts) | Scenes::Reduced(cuts) => cuts,
        _ => return None,
    };
    let split = cuts.partition_point(|cut| cut.get() < before);
    let cut = split
        .checked_sub(1)
        .and_then(|index| cuts.get(index))
        .into_iter()
        .chain(cuts.get(split))
        .filter(|cut| {
            let t = cut.get() as i64;
            let point = before as i64;
            t - i64::from(profile.timing.scene_before_ms) < point
                && point < t + i64::from(profile.timing.scene_after_ms)
        })
        .min_by_key(|cut| cut.get().abs_diff(before))?;
    let side = |value: u64| if value < cut.get() { "before" } else { "after" };
    let old = before.abs_diff(cut.get());
    let new = after.abs_diff(cut.get());
    let text = if side(before) == side(after) {
        format!("{label} {old} → {new} ms {} the cut", side(after))
    } else {
        format!(
            "{label} {old} ms {} → {new} ms {} the cut",
            side(before),
            side(after)
        )
    };
    Some((
        EffectSegment::plain(text),
        subtitle_formats::srt::format_time(*cut),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_use_the_full_precision_rate() {
        let mut effect = Effect {
            index: 0,
            rule: Rule::CpsHigh,
            limit: 20,
            before: Some(19.),
            after: Some(20.),
        };
        assert_eq!(speed(&effect, 0).unwrap().text, "CPS 19.0 → 20.0");
        effect.rule = Rule::RsHigh;
        assert!(speed(&effect, 0).unwrap().warning);
        effect.after = None;
        assert_eq!(speed(&effect, 0).unwrap().text, "RS 19.0 → —");
        effect.after = Some(19.09);
        assert!(speed(&effect, 1).is_none());
    }
}
