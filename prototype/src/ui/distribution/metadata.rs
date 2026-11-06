pub(super) fn insert_value(values: &mut Vec<String>, value: String) {
    let identity = value.to_lowercase();
    if !values
        .iter()
        .any(|selected| selected.to_lowercase() == identity)
    {
        values.push(value);
    }
}

pub(super) fn belongs_to(character: &str, copyright: &str) -> bool {
    character
        .to_lowercase()
        .ends_with(&format!(" ({})", copyright.to_lowercase()))
}

pub(super) fn prune_characters(values: &mut [Vec<String>; 3]) {
    let [copyrights, _, characters] = values;
    characters.retain(|character| {
        copyrights
            .iter()
            .any(|copyright| belongs_to(character, copyright))
    });
}

pub(super) fn entry_values(entry: &crate::catalog::Entry) -> [Vec<String>; 3] {
    let mut characters = entry
        .details
        .character_copyrights
        .iter()
        .map(|(name, copyright)| format!("{name} ({copyright})"))
        .collect::<Vec<_>>();
    if characters.is_empty() {
        for name in entry.characters.split(", ").filter(|name| !name.is_empty()) {
            if let Some(copyright) = entry.details.copyrights.first() {
                characters.push(format!("{name} ({copyright})"));
            }
        }
    }

    [
        entry.details.copyrights.clone(),
        entry.tags.clone(),
        characters,
    ]
}

pub(super) fn apply_entry(
    entry: &mut crate::catalog::Entry,
    values: &[Vec<String>; 3],
    artist: &str,
    title: &str,
) {
    entry.tags = values[1].clone();
    entry.details.copyrights = values[0].clone();
    entry.details.character_copyrights = values[2]
        .iter()
        .filter_map(|value| {
            let (name, copyright) = character_parts(value, &values[0])?;
            Some((name.to_string(), copyright.to_string()))
        })
        .collect();
    entry.characters = entry
        .details
        .character_copyrights
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    entry.details.artist = Some(artist.to_string());
    if entry.video && !title.is_empty() {
        entry.title = title.to_string();
    }
}

pub(super) fn character_parts<'a>(
    value: &'a str,
    copyrights: &'a [String],
) -> Option<(&'a str, &'a str)> {
    let body = value.strip_suffix(')')?;
    body.match_indices(" (").find_map(|(index, _)| {
        let candidate = &body[index + 2..];
        copyrights
            .iter()
            .find(|copyright| copyright.to_lowercase() == candidate.to_lowercase())
            .map(|copyright| (&body[..index], copyright.as_str()))
    })
}

#[cfg(test)]
mod tests;
