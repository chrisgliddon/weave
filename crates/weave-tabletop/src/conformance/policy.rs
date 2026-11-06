//! Machine-readable public-source policy checked by the adapter conformance suite.
//!
//! The suite never contains adapter-specific branches. Instead it compares each manifest against
//! one entry in [`AdapterSourcePolicy`], so a release reviewer changes data in this module rather
//! than test logic.

use crate::AdapterSourceClass;

/// One exact file the adapter must retain, declare, and describe with the same digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyFile {
    /// Project-relative artifact name.
    pub path: &'static str,
    /// Exact SHA-256 the manifest must declare.
    pub sha256: &'static str,
}

/// Complete public-source expectations for one exact adapter release.
///
/// Adapters are identified by their global `id`; the policy intentionally does not key on module
/// names so a fourth adapter joins by adding one row to [`standard_source_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterSourcePolicy {
    /// Stable adapter identity; must equal [`crate::AdapterManifest::id`].
    pub adapter_id: &'static str,
    /// Adapter namespace that must appear as a segment of the global identity.
    pub namespace: &'static str,
    /// Exact licence identifier permitted for this source class.
    pub license: &'static str,
    /// Source class that must be declared by the manifest.
    pub source_class: AdapterSourceClass,
    /// Exact primary source digest the manifest must reproduce.
    pub source_sha256: &'static str,
    /// Retained licence text the adapter must redistribute with the exact digest.
    pub required_license_text: PolicyFile,
    /// Additional notice files the adapter must declare with exact digests.
    pub required_notice_files: &'static [PolicyFile],
    /// Exact notice sentences the manifest must retain.
    pub required_notices: &'static [&'static str],
    /// Exact names that may never be redistributed by this adapter.
    pub forbidden_files: &'static [&'static str],
}

/// Source classes paired with the only licence identifiers accepted for each class.
pub const LICENSE_ALLOWLIST: &[(AdapterSourceClass, &str)] = &[
    (AdapterSourceClass::Original, "MIT"),
    (AdapterSourceClass::VerifiedCc0, "CC0-1.0"),
    (AdapterSourceClass::SeparatelyLicensedApache2, "Apache-2.0"),
];

/// File extensions that mark layout, artwork, or font material no adapter may redistribute.
pub const FORBIDDEN_REDISTRIBUTED_EXTENSIONS: &[&str] = &[
    ".pdf", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".indd", ".ttf", ".otf", ".woff",
    ".woff2",
];

const PLUG_AND_PLAY_NOTICES: &[&str] = &[
    "The official PDFs are hash-pinned inputs and are not redistributed with Weave.",
    "Factual compatibility naming does not imply sponsorship or endorsement.",
];

const DUNGEONPUNK_NOTICES: &[&str] = &[
    "The official release page is https://acegiak.itch.io/dungeonpunk.",
    "The official PDF and text exports are audit inputs and are not redistributed with Weave.",
    "The community wiki and every third-party supplement remain outside this adapter's reviewed source boundary.",
];

const FREEHACK_NOTICES: &[&str] = &[
    "Only hashes and independently authored behavior are bundled; the upstream PDF, icon, stylesheet, and page layout are excluded.",
    "Freehack public payloads must be produced by the dedicated projection API; generic receipt projection retains host-event envelopes and is not a player transport.",
];

/// Reviewed Plug-And-Play source policy.
pub const PLUG_AND_PLAY_SOURCE_POLICY: AdapterSourcePolicy = AdapterSourcePolicy {
    adapter_id: crate::PLUG_AND_PLAY_ADAPTER_ID,
    namespace: "plug_and_play",
    license: "CC0-1.0",
    source_class: AdapterSourceClass::VerifiedCc0,
    source_sha256: crate::PLUG_AND_PLAY_RULES_SHA256,
    required_license_text: PolicyFile {
        path: "LICENSE-CC0-1.0.txt",
        sha256: crate::CC0_1_0_LEGAL_CODE_SHA256,
    },
    required_notice_files: &[],
    required_notices: PLUG_AND_PLAY_NOTICES,
    forbidden_files: &[
        "Plug-And-Play Rules.pdf",
        "Plug-And-Play Character Sheet.pdf",
    ],
};

/// Reviewed Dungeonpunk source policy.
pub const DUNGEONPUNK_SOURCE_POLICY: AdapterSourcePolicy = AdapterSourcePolicy {
    adapter_id: crate::DUNGEONPUNK_ADAPTER_ID,
    namespace: "dungeonpunk",
    license: "CC0-1.0",
    source_class: AdapterSourceClass::VerifiedCc0,
    source_sha256: crate::DUNGEONPUNK_PDF_SHA256,
    required_license_text: PolicyFile {
        path: "LICENSE-CC0-1.0.txt",
        sha256: crate::CC0_1_0_LEGAL_CODE_SHA256,
    },
    required_notice_files: &[],
    required_notices: DUNGEONPUNK_NOTICES,
    forbidden_files: &["dungeonpunk-google-doc.pdf", "dungeonpunk-google-doc.txt"],
};

/// Reviewed Freehack source policy.
pub const FREEHACK_SOURCE_POLICY: AdapterSourcePolicy = AdapterSourcePolicy {
    adapter_id: crate::FREEHACK_ADAPTER_ID,
    namespace: "freehack",
    license: "CC0-1.0",
    source_class: AdapterSourceClass::VerifiedCc0,
    source_sha256: crate::FREEHACK_PDF_SHA256,
    required_license_text: PolicyFile {
        path: "LICENSE-CC0-1.0.txt",
        sha256: crate::FREEHACK_CC0_1_0_LEGAL_CODE_SHA256,
    },
    required_notice_files: &[],
    required_notices: FREEHACK_NOTICES,
    forbidden_files: &[
        "freehack.pdf",
        "src/freehack.md",
        "src/freehack.yml",
        "tools/roll.py",
    ],
};

/// Reviewed Wicked Ones source policy.
pub const WICKED_ONES_SOURCE_POLICY: AdapterSourcePolicy = AdapterSourcePolicy {
    adapter_id: crate::WICKED_ONES_ADAPTER_ID,
    namespace: "wicked_ones",
    license: "CC0-1.0",
    source_class: AdapterSourceClass::VerifiedCc0,
    source_sha256: crate::WICKED_ONES_SOURCE_RECORD_SHA256,
    required_license_text: PolicyFile {
        path: "LICENSE-CC0-1.0.txt",
        sha256: crate::WICKED_ONES_CC0_LEGAL_CODE_SHA256,
    },
    required_notice_files: &[],
    required_notices: crate::WICKED_ONES_NOTICES,
    forbidden_files: &["War for the Overworld", "Undead Awakening"],
};

/// Registry of reviewed source policies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePolicy {
    entries: Vec<AdapterSourcePolicy>,
}

impl SourcePolicy {
    /// Build a policy from an explicit entry table.
    #[must_use]
    pub fn new(entries: Vec<AdapterSourcePolicy>) -> Self {
        Self { entries }
    }

    /// Look up the exact policy for one adapter identity.
    #[must_use]
    pub fn entry(&self, adapter_id: &str) -> Option<&AdapterSourcePolicy> {
        self.entries
            .iter()
            .find(|entry| entry.adapter_id == adapter_id)
    }

    /// Every reviewed entry, in registration order.
    pub fn entries(&self) -> impl Iterator<Item = &AdapterSourcePolicy> {
        self.entries.iter()
    }
}

/// Policy table for every adapter shipped in this build.
#[must_use]
pub fn standard_source_policy() -> SourcePolicy {
    SourcePolicy::new(vec![
        PLUG_AND_PLAY_SOURCE_POLICY,
        DUNGEONPUNK_SOURCE_POLICY,
        FREEHACK_SOURCE_POLICY,
        WICKED_ONES_SOURCE_POLICY,
    ])
}
