//! Portable Agent model-catalog semantics mirrored from Desktop current main.
//! Authenticated USER_AVAILABLE discovery remains a Host responsibility.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandModelCatalogValue {
    pub value: String,
    pub display_name: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandModelCatalogParameterType { Boolean, Enum }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandModelCatalogParameter {
    pub id: String,
    pub name: Option<String>,
    pub parameter_type: SandModelCatalogParameterType,
    pub values: Vec<SandModelCatalogValue>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandModelCatalogParameterValue { pub id: String, pub value: String }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandModelCatalogEntry {
    pub id: String,
    pub display_name: Option<String>,
    pub aliases: Vec<String>,
    pub params: Vec<SandModelCatalogParameter>,
    pub variants: Vec<Vec<SandModelCatalogParameterValue>>,
}

pub fn find_catalog_entry<'a>(
    catalog: &'a [SandModelCatalogEntry],
    model_id: &str,
) -> Option<&'a SandModelCatalogEntry> {
    let needle = model_id.trim().to_ascii_lowercase();
    if needle.is_empty() { return None; }
    catalog.iter().find(|entry| {
        entry.id.to_ascii_lowercase() == needle
            || entry.aliases.iter().any(|alias| alias.to_ascii_lowercase() == needle)
    })
}

fn variant_matches(
    variant: &[SandModelCatalogParameterValue],
    requested: &[(String, String)],
) -> bool {
    requested.iter().all(|(id, value)| {
        variant.iter().any(|candidate| candidate.id == *id && candidate.value == *value)
    })
}

pub fn validate_param_combination(
    entry: &SandModelCatalogEntry,
    params: &[(String, String)],
) -> Option<String> {
    if entry.variants.is_empty() || params.is_empty() { return None; }
    if entry.variants.iter().any(|variant| variant_matches(variant, params)) { return None; }
    let conflicting: Vec<&str> = params.iter().filter_map(|(drop_id, _)| {
        let rest = params.iter().filter(|(id, _)| id != drop_id).cloned().collect::<Vec<_>>();
        entry.variants.iter().any(|variant| variant_matches(variant, &rest))
            .then_some(drop_id.as_str())
    }).collect();
    let requested = params.iter().map(|(id, value)| format!("{id}={value}"))
        .collect::<Vec<_>>().join(", ");
    let hint = if conflicting.len() > 1 {
        format!(" These parameters can't be combined on {}: {}.", entry.id, conflicting.join(", "))
    } else { String::new() };
    Some(format!(
        "{{{requested}}} is not a supported parameter combination for {}.{} Change one of the conflicting parameters and retry (for example, on some models a 1M context can't be combined with the fast tier).",
        entry.id, hint
    ))
}

pub fn validate_model_params(
    entry: &SandModelCatalogEntry,
    params: &[(String, String)],
) -> Vec<String> {
    let mut errors = Vec::new();
    for (id, value) in params {
        let Some(definition) = entry.params.iter().find(|parameter| parameter.id == *id) else {
            let allowed = entry.params.iter().map(|parameter| parameter.id.as_str())
                .collect::<Vec<_>>().join(", ");
            errors.push(if allowed.is_empty() {
                format!("'{id}' is not a parameter of {}. This model takes no parameters.", entry.id)
            } else {
                format!("'{id}' is not a parameter of {}. Allowed parameters: {allowed}.", entry.id)
            });
            continue;
        };
        if !definition.values.iter().any(|candidate| candidate.value == *value) {
            let allowed = definition.values.iter().map(|candidate| candidate.value.as_str())
                .collect::<Vec<_>>().join(", ");
            errors.push(format!(
                "'{value}' is not a valid value for '{id}' on {}. Allowed: {allowed}.",
                entry.id
            ));
        }
    }
    if errors.is_empty() {
        if let Some(error) = validate_param_combination(entry, params) { errors.push(error); }
    }
    errors
}

pub fn describe_param_incompatibilities(entry: &SandModelCatalogEntry) -> Vec<String> {
    if entry.variants.is_empty() { return Vec::new(); }
    let mut present = Vec::<SandModelCatalogParameterValue>::new();
    for variant in &entry.variants {
        for value in variant {
            if !present.contains(value) { present.push(value.clone()); }
        }
    }
    let mut notes = Vec::new();
    for (index, left) in present.iter().enumerate() {
        for right in present.iter().skip(index + 1) {
            if left.id == right.id { continue; }
            let co_occurs = entry.variants.iter().any(|variant| {
                variant.iter().any(|v| v.id == left.id && v.value == left.value)
                    && variant.iter().any(|v| v.id == right.id && v.value == right.value)
            });
            if !co_occurs {
                notes.push(format!(
                    "{}={} cannot be combined with {}={}",
                    left.id, left.value, right.id, right.value
                ));
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> SandModelCatalogEntry {
        SandModelCatalogEntry {
            id: "grok-4.5".into(),
            display_name: Some("Grok 4.5".into()),
            aliases: vec!["GROK".into()],
            params: vec![
                SandModelCatalogParameter {
                    id: "fast".into(), name: Some("Fast".into()),
                    parameter_type: SandModelCatalogParameterType::Boolean,
                    values: vec![
                        SandModelCatalogValue { value: "true".into(), display_name: None },
                        SandModelCatalogValue { value: "false".into(), display_name: None },
                    ],
                },
                SandModelCatalogParameter {
                    id: "context".into(), name: None,
                    parameter_type: SandModelCatalogParameterType::Enum,
                    values: vec![
                        SandModelCatalogValue { value: "normal".into(), display_name: None },
                        SandModelCatalogValue { value: "1m".into(), display_name: Some("1M".into()) },
                    ],
                },
            ],
            variants: vec![
                vec![
                    SandModelCatalogParameterValue { id: "fast".into(), value: "true".into() },
                    SandModelCatalogParameterValue { id: "context".into(), value: "normal".into() },
                ],
                vec![
                    SandModelCatalogParameterValue { id: "fast".into(), value: "false".into() },
                    SandModelCatalogParameterValue { id: "context".into(), value: "normal".into() },
                ],
                vec![
                    SandModelCatalogParameterValue { id: "fast".into(), value: "false".into() },
                    SandModelCatalogParameterValue { id: "context".into(), value: "1m".into() },
                ],
            ],
        }
    }
    #[test]
    fn lookup_is_trimmed_case_insensitive_and_alias_aware() {
        let catalog = vec![entry()];
        assert_eq!(find_catalog_entry(&catalog, "  GROK-4.5  ").unwrap().id, "grok-4.5");
        assert_eq!(find_catalog_entry(&catalog, "grok").unwrap().id, "grok-4.5");
        assert!(find_catalog_entry(&catalog, "unknown").is_none());
    }
    #[test]
    fn validation_rejects_unknown_values_and_impossible_variants() {
        let entry = entry();
        assert!(validate_model_params(
            &entry,
            &[("fast".into(), "true".into()), ("context".into(), "normal".into())],
        ).is_empty());
        assert!(validate_model_params(&entry, &[("fast".into(), "maybe".into())])[0]
            .contains("Allowed: true, false"));
        let bad = validate_model_params(
            &entry,
            &[("fast".into(), "true".into()), ("context".into(), "1m".into())],
        );
        assert_eq!(bad.len(), 1);
        assert!(bad[0].contains("fast, context"));
    }
    #[test]
    fn incompatibilities_come_from_variant_truth() {
        let notes = describe_param_incompatibilities(&entry());
        assert!(notes.contains(&"fast=true cannot be combined with context=1m".to_string()));
        assert!(!notes.iter().any(|n| n.contains("fast=false") && n.contains("context=1m")));
    }
}
