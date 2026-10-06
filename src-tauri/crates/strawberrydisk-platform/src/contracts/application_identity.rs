use serde::{Deserialize, Serialize};

/// Descriptive evidence only; metadata and certificate names do not establish product safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "platform",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ApplicationIdentityMetadata {
    Macos {
        #[serde(skip_serializing_if = "Option::is_none")]
        bundle_identifier: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        product_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        signing: ApplicationSigningMetadata,
    },
    Windows {
        #[serde(skip_serializing_if = "Option::is_none")]
        product_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        company_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        package_identity: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationSigningMetadata {
    pub kind: ApplicationSigningKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_identifier: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplicationSigningKind {
    Certificate,
    AdHoc,
    Unsigned,
    Unavailable,
}
