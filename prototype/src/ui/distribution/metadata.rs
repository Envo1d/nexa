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

#[cfg(test)]
mod tests;
