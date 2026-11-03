// Exact sizing rules from MediaCollection.vue at the reference 13px root size.
pub(super) const GAP: f32 = 13.0;
pub(super) const TITLEBAR_HEIGHT: f32 = 56.0;
pub(super) const DRAWER_TOP: f32 = TITLEBAR_HEIGHT + 59.0;
const GRID_PADDING: f32 = 3.9;
const MIN_CARD_WIDTH: f32 = 227.5;
const CARD_BODY_HEIGHT: f32 = 74.75;

pub(super) fn filter_width(viewport: f32) -> f32 {
    if viewport >= 1450.0 {
        244.0
    } else if viewport <= 1100.0 {
        196.0
    } else {
        220.0
    }
}

pub(super) fn full_duration(duration: Option<&str>) -> String {
    match duration {
        Some(value) if value.matches(':').count() == 1 => format!("00:{value}"),
        Some(value) => value.to_owned(),
        None => "Недоступно".into(),
    }
}

pub(super) fn compact_date(date: Option<&str>) -> String {
    let Some(date) = date else {
        return "Дата недоступна".into();
    };
    let Some((day, time)) = date.split_once(' ') else {
        return date.into();
    };
    let parts: Vec<_> = day.split('.').collect();
    let month = parts.get(1).and_then(|value| value.parse::<usize>().ok());
    let months = [
        "янв.",
        "февр.",
        "мар.",
        "апр.",
        "мая",
        "июн.",
        "июл.",
        "авг.",
        "сент.",
        "окт.",
        "нояб.",
        "дек.",
    ];

    match month
        .and_then(|month| month.checked_sub(1))
        .and_then(|month| months.get(month))
    {
        Some(month) => format!("{} {month}, {time}", parts[0]),
        None => date.into(),
    }
}

pub(super) struct CollectionLayout {
    pub columns: usize,
    pub card_width: f32,
    pub preview_height: f32,
    pub row_height: f32,
}

impl CollectionLayout {
    pub fn new(width: f32, list: bool) -> Self {
        let width = width.max(1.0);
        let columns = if list {
            1
        } else {
            ((width - GRID_PADDING + GAP) / (MIN_CARD_WIDTH + GAP))
                .floor()
                .max(1.0) as usize
        };
        let card_width =
            ((width - GRID_PADDING - GAP * (columns - 1) as f32) / columns as f32).max(0.0);
        let preview_height = (card_width - 2.0).max(0.0) * 9.0 / 16.0;
        let row_height = if list {
            88.0
        } else {
            preview_height + CARD_BODY_HEIGHT + 2.0 + GAP
        };

        Self {
            columns,
            card_width,
            preview_height,
            row_height,
        }
    }
}

#[cfg(test)]
mod tests;
