use serde::Serialize;

#[derive(Serialize)]
pub struct MobileRuleDefinition {
    pub id: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub default_severity: &'static str,
}

pub const MOBILE_RULE_DEFINITIONS: &[MobileRuleDefinition] = &[
    MobileRuleDefinition {
        id: "MOB-PRJ-001",
        category: "project",
        description: "Supported mobile project detection",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-PRJ-002",
        category: "project",
        description: "Project scan coverage and limitations",
        default_severity: "warning",
    },
    MobileRuleDefinition {
        id: "MOB-AND-001",
        category: "android_policy",
        description: "Android target SDK policy",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-AND-002",
        category: "security",
        description: "Android debuggable flag",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-AND-003",
        category: "security",
        description: "Android cleartext traffic",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-AND-004",
        category: "identity",
        description: "Android application identity",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-AND-005",
        category: "signing",
        description: "Android release signing path",
        default_severity: "warning",
    },
    MobileRuleDefinition {
        id: "MOB-AND-006",
        category: "privacy",
        description: "Sensitive Android permission review",
        default_severity: "warning",
    },
    MobileRuleDefinition {
        id: "MOB-IOS-001",
        category: "identity",
        description: "iOS bundle identity",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-IOS-002",
        category: "versioning",
        description: "iOS version/build identity",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-IOS-003",
        category: "privacy",
        description: "Apple privacy manifest validity and coverage",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-IOS-004",
        category: "privacy",
        description: "iOS protected-resource purpose strings",
        default_severity: "warning",
    },
    MobileRuleDefinition {
        id: "MOB-CFG-001",
        category: "configuration",
        description: "Production placeholder detection",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-STR-001",
        category: "store_metadata",
        description: "Required public privacy and support URLs",
        default_severity: "warning",
    },
    MobileRuleDefinition {
        id: "MOB-ART-001",
        category: "artifact",
        description: "Artifact inspection and SHA-256 provenance",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-ART-002",
        category: "identity",
        description: "Packaged application identity",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-ART-003",
        category: "versioning",
        description: "Packaged version/build identity",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-ART-004",
        category: "signing",
        description: "Cryptographic artifact signature verification",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-ART-005",
        category: "android_policy",
        description: "Packaged Android target SDK",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-ART-006",
        category: "privacy",
        description: "Packaged iOS privacy manifest coverage",
        default_severity: "blocker",
    },
    MobileRuleDefinition {
        id: "MOB-EVD-INVALID",
        category: "evidence",
        description: "Evidence schema and trust validation",
        default_severity: "warning",
    },
];
