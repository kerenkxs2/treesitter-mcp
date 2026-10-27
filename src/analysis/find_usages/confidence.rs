use super::*;

pub(super) fn assign_confidence(usages: &mut [UsageRow]) {
    let definitions: Vec<DefinitionSite> = usages
        .iter()
        .filter(|usage| usage.usage_type == "definition")
        .map(DefinitionSite::from)
        .collect();

    for usage in usages.iter_mut() {
        usage.confidence = confidence_for_usage(usage, &definitions).to_string();
    }
}

#[derive(Clone, Copy)]
enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

struct DefinitionSite {
    file: String,
    scope: String,
    owner: Option<String>,
}

impl DefinitionSite {
    fn from(usage: &UsageRow) -> Self {
        Self {
            file: usage.file.clone(),
            scope: usage.scope.clone(),
            owner: definition_owner(usage),
        }
    }
}

fn confidence_for_usage(usage: &UsageRow, definitions: &[DefinitionSite]) -> Confidence {
    if usage.usage_type == "definition" {
        return Confidence::High;
    }

    if definitions.is_empty() {
        return Confidence::Low;
    }

    if let Some(owner_hint) = usage.owner_hint.as_deref() {
        let owner_matches = definitions
            .iter()
            .filter(|definition| {
                owner_matches(definition.owner.as_deref(), owner_hint, &usage.scope)
            })
            .count();
        if owner_matches == 1 {
            return Confidence::High;
        }
    }

    let same_scope_matches = definitions
        .iter()
        .filter(|definition| scope_aligns(&definition.scope, &usage.scope))
        .count();
    if same_scope_matches == 1 {
        return Confidence::High;
    }

    if definitions
        .iter()
        .any(|definition| definition.file == usage.file)
    {
        return Confidence::Medium;
    }

    Confidence::Low
}

pub(super) fn owner_matches(
    definition_owner: Option<&str>,
    owner_hint: &str,
    usage_scope: &str,
) -> bool {
    if owner_hint.eq_ignore_ascii_case("self") || owner_hint.eq_ignore_ascii_case("this") {
        return definition_owner
            .map(|owner| usage_scope.split("::").any(|segment| segment == owner))
            .unwrap_or(false);
    }

    definition_owner
        .map(|owner| owner == owner_hint)
        .unwrap_or(false)
}

pub(super) fn scope_aligns(definition_scope: &str, usage_scope: &str) -> bool {
    let Some(definition_owner) = scope_owner(definition_scope) else {
        return false;
    };

    usage_scope
        .split("::")
        .any(|segment| segment == definition_owner)
}

pub(super) fn scope_owner(scope: &str) -> Option<&str> {
    scope.rsplit("::").nth(1)
}

pub(super) fn definition_owner(usage: &UsageRow) -> Option<String> {
    scope_owner(&usage.scope)
        .map(ToOwned::to_owned)
        .or_else(|| {
            Path::new(&usage.file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToOwned::to_owned)
        })
}
