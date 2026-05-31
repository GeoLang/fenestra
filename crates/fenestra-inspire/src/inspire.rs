//! INSPIRE metadata validation.

/// Validates metadata records against INSPIRE Technical Guidance.
pub struct InspireValidator;

impl InspireValidator {
    /// Validates an ISO 19139 XML metadata document against INSPIRE rules.
    pub fn validate(xml: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Mandatory INSPIRE elements
        if !xml.contains("gmd:identificationInfo") {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                rule: "TG-1.1".to_string(),
                message: "Missing mandatory identificationInfo element".to_string(),
            });
        }

        if !xml.contains("gmd:contact") {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                rule: "TG-1.3".to_string(),
                message: "Missing mandatory contact element".to_string(),
            });
        }

        if !xml.contains("gmd:dateStamp") {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                rule: "TG-1.4".to_string(),
                message: "Missing mandatory dateStamp element".to_string(),
            });
        }

        issues
    }
}

/// A validation issue found in a metadata record.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub rule: String,
    pub message: String,
}

/// Severity level for validation issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}
