//! Advisory rule policy, independently versioned from product and result envelopes.

use serde::Serialize;

pub const CATALOG_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Idiom,
    Complexity,
    Resource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Promotion {
    Advisory,
    CheckerCandidate,
    TheoremCandidate,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rule {
    pub id: &'static str,
    pub group: Group,
    pub promotion: Promotion,
    pub default_severity: &'static str,
    pub description: &'static str,
    pub rationale: &'static str,
}

pub const UNUSED_IMPORT: Rule = Rule {
    id: "unused_import",
    group: Group::Idiom,
    promotion: Promotion::Advisory,
    default_severity: "warning",
    description: "An import has no possible use in the local module.",
    rationale: "Import hygiene is a style preference; unused imports remain valid Qleisli.",
};

pub const REDUNDANT_REPEAT_ONE: Rule = Rule {
    id: "redundant_repeat_one",
    group: Group::Complexity,
    promotion: Promotion::Advisory,
    default_severity: "warning",
    description: "A static repeat or operation repeat has count one.",
    rationale: "Choosing a simpler spelling is advisory; a one-repetition constructor remains valid.",
};

pub const DOUBLE_INVERSE: Rule = Rule {
    id: "double_inverse",
    group: Group::Complexity,
    promotion: Promotion::Advisory,
    default_severity: "warning",
    description: "An operation inverse directly contains another inverse.",
    rationale: "Choosing a simpler spelling is advisory; nested inverses remain valid.",
};

// Sorted by diagnostic ID for deterministic enumeration and lookup.
pub const RULES: [Rule; 3] = [DOUBLE_INVERSE, REDUNDANT_REPEAT_ONE, UNUSED_IMPORT];

pub fn find(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.id == id)
}

pub fn catalog() -> serde_json::Value {
    serde_json::json!({
        "catalog_version": CATALOG_VERSION,
        "groups": [
            {"id": Group::Idiom, "description": "Local conventions, import hygiene, and standard-library idioms."},
            {"id": Group::Complexity, "description": "Unnecessarily complex syntax with a simpler spelling."},
            {"id": Group::Resource, "description": "Advisory resource-use observations; no resource-safety guarantee."},
        ],
        "promotion_policies": [
            {"id": Promotion::Advisory, "description": "Remains advisory; no planned promotion to an acceptance condition or theorem obligation."},
            {"id": Promotion::CheckerCandidate, "description": "May be retired when ordinary Qleisli checking enforces the condition."},
            {"id": Promotion::TheoremCandidate, "description": "May be retired when a Qleisli theorem obligation covers the condition."},
        ],
        "rules": RULES,
    })
}
