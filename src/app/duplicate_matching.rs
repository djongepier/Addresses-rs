// Copyright (c) 2026 D. W. Jongepier
// SPDX-License-Identifier: MIT
// See LICENSE in the project root for license information.

use std::collections::{BTreeSet, HashMap};

use addresses_rs::database::{Contact, ContactFields};

pub(super) struct DuplicateMatch {
    pub(super) first: Contact,
    pub(super) second: Contact,
    pub(super) reasons: Vec<String>,
}

struct MatchData {
    name: Option<String>,
    emails: BTreeSet<String>,
    phones: BTreeSet<String>,
    company: Option<String>,
    locations: BTreeSet<String>,
}

pub(super) fn find_duplicate_matches(contacts: &[Contact]) -> Vec<DuplicateMatch> {
    let data = contacts
        .iter()
        .map(|contact| MatchData::from_fields(&contact.fields))
        .collect::<Vec<_>>();
    let mut groups = HashMap::<String, Vec<usize>>::new();

    for (index, contact) in data.iter().enumerate() {
        add_keys(
            &mut groups,
            contact.emails.iter().map(String::as_str),
            index,
        );
        add_keys(
            &mut groups,
            contact.phones.iter().map(String::as_str),
            index,
        );
        if let Some(name) = contact.name.as_deref()
            && name.chars().count() >= 5
        {
            add_keys(&mut groups, std::iter::once(name), index);
        }
        if let Some(company) = contact.company.as_deref() {
            add_keys(&mut groups, std::iter::once(company), index);
        }
        add_keys(
            &mut groups,
            contact.locations.iter().map(String::as_str),
            index,
        );
    }

    let mut candidates = BTreeSet::new();
    for group in groups.values() {
        for (position, first) in group.iter().enumerate() {
            for second in group.iter().skip(position + 1) {
                candidates.insert((*first, *second));
            }
        }
    }

    let mut matches = Vec::new();
    for (first_index, second_index) in candidates {
        let first = &data[first_index];
        let second = &data[second_index];
        let mut reasons = BTreeSet::new();

        if !first.emails.is_disjoint(&second.emails) {
            reasons.insert("Same email address".to_owned());
        }
        if !first.phones.is_disjoint(&second.phones) {
            reasons.insert("Same phone number".to_owned());
        }

        match (&first.name, &second.name) {
            (Some(first_name), Some(second_name)) if first_name == second_name => {
                reasons.insert("Same full name".to_owned());
            }
            (Some(first_name), Some(second_name)) if names_are_similar(first_name, second_name) => {
                if first.company.is_some() && first.company == second.company {
                    reasons.insert("Similar name and organization".to_owned());
                } else if !first.locations.is_disjoint(&second.locations) {
                    reasons.insert("Similar name and postal address".to_owned());
                }
            }
            _ => {}
        }

        if !reasons.is_empty() {
            matches.push(DuplicateMatch {
                first: contacts[first_index].clone(),
                second: contacts[second_index].clone(),
                reasons: reasons.into_iter().collect(),
            });
        }
    }

    matches
}

impl MatchData {
    fn from_fields(fields: &ContactFields) -> Self {
        let canonical_name = [
            fields.name.prefix.as_deref(),
            fields.name.given_name.as_deref(),
            fields.name.middle_name.as_deref(),
            fields.name.family_name.as_deref(),
            fields.name.suffix.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        let name = normalize_text(if canonical_name.is_empty() {
            fields
                .name
                .display_name
                .as_deref()
                .or(fields.name.file_as.as_deref())
                .or(fields.name.nickname.as_deref())
                .unwrap_or_default()
        } else {
            &canonical_name
        });

        let emails = fields
            .emails
            .iter()
            .filter_map(|email| {
                let email = email.value.trim().to_lowercase();
                (!email.is_empty()).then_some(email)
            })
            .collect();
        let phones = fields
            .phones
            .iter()
            .filter_map(|phone| {
                let digits = phone
                    .value
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>();
                (digits.len() >= 7).then_some(digits)
            })
            .collect();
        let company = fields
            .organization
            .company
            .as_deref()
            .and_then(normalize_text);
        let locations = fields
            .addresses
            .iter()
            .filter_map(|address| {
                let postal_code = address.postal_code.as_deref().and_then(normalize_text)?;
                let city = address.city.as_deref().and_then(normalize_text)?;
                let country = address
                    .country
                    .as_deref()
                    .and_then(normalize_text)
                    .unwrap_or_default();
                Some(format!("{postal_code}|{city}|{country}"))
            })
            .collect();

        Self {
            name,
            emails,
            phones,
            company,
            locations,
        }
    }
}

fn add_keys<'a>(
    groups: &mut HashMap<String, Vec<usize>>,
    keys: impl Iterator<Item = &'a str>,
    index: usize,
) {
    for key in keys {
        groups.entry(key.to_owned()).or_default().push(index);
    }
}

fn normalize_text(value: &str) -> Option<String> {
    let normalized = value
        .trim()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    (!normalized.is_empty()).then_some(normalized)
}

fn names_are_similar(first: &str, second: &str) -> bool {
    let first = first.chars().collect::<Vec<_>>();
    let second = second.chars().collect::<Vec<_>>();
    let longest = first.len().max(second.len());
    if !(8..=128).contains(&longest) {
        return false;
    }

    levenshtein_distance(&first, &second) * 10 <= longest
}

fn levenshtein_distance(first: &[char], second: &[char]) -> usize {
    let (shorter, longer) = if first.len() <= second.len() {
        (first, second)
    } else {
        (second, first)
    };
    let mut previous = (0..=shorter.len()).collect::<Vec<_>>();
    let mut current = vec![0; shorter.len() + 1];

    for (longer_index, longer_character) in longer.iter().enumerate() {
        current[0] = longer_index + 1;
        for (shorter_index, shorter_character) in shorter.iter().enumerate() {
            current[shorter_index + 1] = (previous[shorter_index + 1] + 1)
                .min(current[shorter_index] + 1)
                .min(previous[shorter_index] + usize::from(longer_character != shorter_character));
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[shorter.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use addresses_rs::database::{ContactName, LabeledValue, Organization, PostalAddress};

    fn contact(id: i64, fields: ContactFields) -> Contact {
        Contact {
            id,
            address_book_id: id,
            fields,
        }
    }

    #[test]
    fn matches_emails_and_phones_after_normalization() {
        let first = contact(
            1,
            ContactFields {
                emails: vec![LabeledValue {
                    label: None,
                    value: " ADA@EXAMPLE.COM ".to_owned(),
                }],
                ..ContactFields::default()
            },
        );
        let second = contact(
            2,
            ContactFields {
                emails: vec![LabeledValue {
                    label: None,
                    value: "ada@example.com".to_owned(),
                }],
                phones: vec![LabeledValue {
                    label: None,
                    value: "+1 (555) 010-1234".to_owned(),
                }],
                ..ContactFields::default()
            },
        );
        let third = contact(
            3,
            ContactFields {
                phones: vec![LabeledValue {
                    label: None,
                    value: "1 555 010 1234".to_owned(),
                }],
                ..ContactFields::default()
            },
        );

        let matches = find_duplicate_matches(&[first, second, third]);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].reasons, ["Same email address"]);
        assert_eq!(matches[1].reasons, ["Same phone number"]);
    }

    #[test]
    fn matches_exact_names_and_corroborated_similar_names() {
        let same_name = contact(
            1,
            ContactFields {
                name: ContactName {
                    display_name: Some("Ada Lovelace".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
        );
        let same_name_other_book = contact(
            2,
            ContactFields {
                name: ContactName {
                    given_name: Some("Ada".to_owned()),
                    family_name: Some("Lovelace".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
        );
        let similar_name = contact(
            3,
            ContactFields {
                name: ContactName {
                    display_name: Some("Ada Lovelace".to_owned()),
                    ..ContactName::default()
                },
                organization: Organization {
                    company: Some("Analytical Engines".to_owned()),
                    ..Organization::default()
                },
                ..ContactFields::default()
            },
        );
        let typo_name = contact(
            4,
            ContactFields {
                name: ContactName {
                    display_name: Some("Ada Lovelacee".to_owned()),
                    ..ContactName::default()
                },
                organization: Organization {
                    company: Some("Analytical Engines".to_owned()),
                    ..Organization::default()
                },
                ..ContactFields::default()
            },
        );

        let matches =
            find_duplicate_matches(&[same_name, same_name_other_book, similar_name, typo_name]);

        assert!(matches.iter().any(|pair| {
            pair.first.id == 1 && pair.second.id == 2 && pair.reasons == ["Same full name"]
        }));
        assert!(matches.iter().any(|pair| {
            pair.first.id == 3
                && pair.second.id == 4
                && pair.reasons == ["Similar name and organization"]
        }));
    }

    #[test]
    fn does_not_match_similar_names_without_corroborating_data() {
        let first = contact(
            1,
            ContactFields {
                name: ContactName {
                    display_name: Some("Jonathan Smith".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
        );
        let second = contact(
            2,
            ContactFields {
                name: ContactName {
                    display_name: Some("Johnathan Smith".to_owned()),
                    ..ContactName::default()
                },
                ..ContactFields::default()
            },
        );

        assert!(find_duplicate_matches(&[first, second]).is_empty());
    }

    #[test]
    fn does_not_match_short_phone_values() {
        let first = contact(
            1,
            ContactFields {
                phones: vec![LabeledValue {
                    label: None,
                    value: "123-456".to_owned(),
                }],
                ..ContactFields::default()
            },
        );
        let second = contact(
            2,
            ContactFields {
                phones: vec![LabeledValue {
                    label: None,
                    value: "123456".to_owned(),
                }],
                ..ContactFields::default()
            },
        );

        assert!(find_duplicate_matches(&[first, second]).is_empty());
    }

    #[test]
    fn email_matching_preserves_meaningful_punctuation() {
        let contact_with_email = |id, email: &str| {
            contact(
                id,
                ContactFields {
                    emails: vec![LabeledValue {
                        label: None,
                        value: email.to_owned(),
                    }],
                    ..ContactFields::default()
                },
            )
        };

        assert!(
            find_duplicate_matches(&[
                contact_with_email(1, "a.b@example.com"),
                contact_with_email(2, "ab@example.com"),
            ])
            .is_empty()
        );
    }

    #[test]
    fn similar_names_can_be_corroborated_by_postal_address() {
        let fields = |name: &str| ContactFields {
            name: ContactName {
                display_name: Some(name.to_owned()),
                ..ContactName::default()
            },
            addresses: vec![PostalAddress {
                postal_code: Some("12345".to_owned()),
                city: Some("Amsterdam".to_owned()),
                ..PostalAddress::default()
            }],
            ..ContactFields::default()
        };
        let matches = find_duplicate_matches(&[
            contact(1, fields("Jonathan Smith")),
            contact(2, fields("Johnathan Smith")),
        ]);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].reasons, ["Similar name and postal address"]);
    }
}
