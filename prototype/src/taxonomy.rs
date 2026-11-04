use std::collections::{BTreeMap, BTreeSet};

use crate::{catalog::Entry, naming};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Category {
    Copyright,
    Artist,
    Character,
    Tag,
}

impl Category {
    pub(crate) const ALL: [Self; 4] = [Self::Copyright, Self::Artist, Self::Character, Self::Tag];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Copyright => "Произведения",
            Self::Artist => "Авторы",
            Self::Character => "Персонажи",
            Self::Tag => "Теги",
        }
    }

    pub(crate) fn singular(self) -> &'static str {
        match self {
            Self::Copyright => "Произведение",
            Self::Artist => "Автор",
            Self::Character => "Персонаж",
            Self::Tag => "Тег",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Copyright => "copyright",
            Self::Artist => "artist",
            Self::Character => "person",
            Self::Tag => "tag",
        }
    }

    pub(crate) fn color(self) -> u32 {
        match self {
            Self::Copyright => 0xe7bf7e,
            Self::Artist => 0x80b8ef,
            Self::Character => 0xe49acb,
            Self::Tag => 0xa786ff,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub id: usize,
    pub category: Category,
    pub name: String,
    pub copyright: String,
    pub count: usize,
}

#[derive(Default)]
pub(crate) struct Catalog {
    pub records: Vec<Record>,
    next_id: usize,
}

fn identities(entry: &Entry, category: Category) -> Vec<(String, String)> {
    match category {
        Category::Copyright => entry
            .details
            .copyrights
            .iter()
            .map(|s| (s.clone(), String::new()))
            .collect(),
        Category::Artist => entry
            .details
            .artist
            .iter()
            .map(|s| (s.clone(), String::new()))
            .collect(),
        Category::Tag => entry
            .tags
            .iter()
            .map(|s| (s.clone(), String::new()))
            .collect(),
        Category::Character => entry.details.character_copyrights.clone(),
    }
}

impl Catalog {
    pub(crate) fn refresh<'a>(&mut self, entries: impl Iterator<Item = &'a Entry>) {
        let mut counts = BTreeMap::new();
        for entry in entries {
            for category in Category::ALL {
                let unique: BTreeSet<_> = identities(entry, category).into_iter().collect();
                for (name, copyright) in unique {
                    *counts.entry((category, name, copyright)).or_insert(0) += 1;
                }
            }
        }

        for record in &mut self.records {
            record.count = counts
                .remove(&(
                    record.category,
                    record.name.clone(),
                    record.copyright.clone(),
                ))
                .unwrap_or(0);
        }
        for ((category, name, copyright), count) in counts {
            self.records.push(Record {
                id: self.next_id,
                category,
                name,
                copyright,
                count,
            });
            self.next_id += 1;
        }
        self.records.sort_by(|a, b| {
            naming::compare_names(&a.name, &b.name)
                .then_with(|| naming::compare_names(&a.copyright, &b.copyright))
        });
    }

    pub(crate) fn repeated(&self) -> BTreeSet<String> {
        let mut names: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for record in self
            .records
            .iter()
            .filter(|r| r.category == Category::Character)
        {
            names
                .entry(naming::name_key(&record.name))
                .or_default()
                .insert(&record.copyright);
        }

        names
            .into_iter()
            .filter(|(_, copyrights)| copyrights.len() > 1)
            .map(|(name, _)| name)
            .collect()
    }

    pub(crate) fn filtered(
        &self,
        category: Category,
        query: &str,
        links: Option<usize>,
        repeated: bool,
    ) -> Vec<usize> {
        let query = naming::name_key(query.trim());
        let duplicates = self.repeated();
        let mut values: Vec<_> = self
            .records
            .iter()
            .filter(|r| {
                r.category == category
                    && links.is_none_or(|count| count == r.count)
                    && (!repeated
                        || category != Category::Character
                        || duplicates.contains(&naming::name_key(&r.name)))
                    && naming::name_key(&r.name).contains(&query)
            })
            .collect();
        values.sort_by_key(|r| !naming::name_key(&r.name).starts_with(&query));

        values.into_iter().map(|r| r.id).collect()
    }

    pub(crate) fn apply(
        &mut self,
        id: usize,
        replacement: Option<(&str, &str)>,
        main: &mut Vec<Entry>,
        incoming: &mut Vec<Entry>,
    ) -> Result<usize, &'static str> {
        let source = self
            .records
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or("Запись больше не существует.")?;
        let next = replacement.map(|(name, copyright)| (name.trim(), copyright));
        if let Some((name, copyright)) = next {
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err("Введите непустое имя без управляющих символов.");
            }
            if source.category == Category::Character
                && !self
                    .records
                    .iter()
                    .any(|r| r.category == Category::Copyright && r.name == copyright)
            {
                return Err("Выберите произведение персонажа.");
            }
            if self.records.iter().any(|r| {
                r.id != id
                    && r.category == source.category
                    && naming::name_key(&r.name) == naming::name_key(name)
                    && (source.category != Category::Character || r.copyright == copyright)
            }) {
                return Err("Такое имя уже существует в справочнике.");
            }
        }

        let affected: Vec<_> = main
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                identities(entry, source.category)
                    .contains(&(source.name.clone(), source.copyright.clone()))
            })
            .map(|(index, _)| index)
            .collect();
        for entry in main.iter_mut().chain(incoming.iter_mut()) {
            update_entry(entry, &source, next);
        }

        let mut queued = 0;
        if next.is_none() && matches!(source.category, Category::Copyright | Category::Artist) {
            for index in affected.into_iter().rev() {
                incoming.push(main.remove(index));
                queued += 1;
            }
        }

        if source.category == Category::Copyright {
            self.records.retain_mut(|r| {
                if r.category == Category::Character && r.copyright == source.name {
                    if let Some((name, _)) = next {
                        r.copyright = name.to_owned();
                    } else {
                        return false;
                    }
                }
                true
            });
        }
        if let Some((name, copyright)) = next {
            let record = self
                .records
                .iter_mut()
                .find(|r| r.id == id)
                .expect("validated record");
            record.name = name.to_owned();
            if source.category == Category::Character {
                record.copyright = copyright.to_owned();
            }
        } else {
            self.records.retain(|r| r.id != id);
        }
        self.refresh(main.iter().chain(incoming.iter()));

        Ok(queued)
    }
}

fn replace(values: &mut Vec<String>, old: &str, next: Option<&str>) {
    *values = values
        .iter()
        .flat_map(|value| {
            if value == old {
                next.map(str::to_owned)
            } else {
                Some(value.clone())
            }
        })
        .collect();
    let mut seen = BTreeSet::new();
    values.retain(|s| seen.insert(s.clone()));
}

fn update_entry(entry: &mut Entry, source: &Record, next: Option<(&str, &str)>) {
    let previous_characters = entry.characters.clone();
    let name = next.map(|(name, _)| name);
    match source.category {
        Category::Tag => replace(&mut entry.tags, &source.name, name),
        Category::Artist => {
            if entry.details.artist.as_deref() == Some(&source.name) {
                entry.details.artist = name.map(str::to_owned);
            }
        }
        Category::Copyright => {
            replace(&mut entry.details.copyrights, &source.name, name);
            entry
                .details
                .character_copyrights
                .retain_mut(|(_, copyright)| {
                    if *copyright != source.name {
                        return true;
                    }
                    if let Some(name) = name {
                        *copyright = name.to_owned();
                        true
                    } else {
                        false
                    }
                });
        }
        Category::Character => {
            let linked = entry
                .details
                .character_copyrights
                .contains(&(source.name.clone(), source.copyright.clone()));
            entry
                .details
                .character_copyrights
                .retain_mut(|(name, copyright)| {
                    if *name != source.name || *copyright != source.copyright {
                        return true;
                    }
                    if let Some((next_name, target)) = next {
                        *name = next_name.to_owned();
                        *copyright = target.to_owned();
                        true
                    } else {
                        false
                    }
                });
            if let Some((_, target)) = next.filter(|_| linked) {
                if !entry.details.copyrights.iter().any(|s| s == target) {
                    entry.details.copyrights.push(target.to_owned());
                }
                let uses_previous = entry
                    .details
                    .character_copyrights
                    .iter()
                    .any(|(_, copyright)| *copyright == source.copyright);
                if target != source.copyright && !uses_previous {
                    entry.details.copyrights.retain(|s| *s != source.copyright);
                }
            }
        }
    }

    let mut seen = BTreeSet::new();
    let names: Vec<_> = entry
        .details
        .character_copyrights
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| seen.insert(*name))
        .collect();
    entry.characters = names.join(", ");
    if entry.demo && !entry.video && entry.title == previous_characters {
        entry.title = if entry.characters.is_empty() {
            entry.details.filename.clone()
        } else {
            entry.characters.clone()
        };
    }
}

#[cfg(test)]
mod tests;
