// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed TED domain — notice identifier + procurement notice record.

use serde::{Deserialize, Serialize};

use crate::error::TedError;

/// TED publication identifier — the canonical form is
/// `<sequence>-<year>` (e.g., `"123456-2026"`). The port validates
/// the shape at the boundary; the live provider's 404 is the
/// authoritative answer to "does this notice exist?".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TedNoticeId(String);

impl TedNoticeId {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, TedError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(TedError::InvalidNoticeId("empty".into()));
        }
        let parts: Vec<&str> = raw.split('-').collect();
        if parts.len() != 2 {
            return Err(TedError::InvalidNoticeId(format!(
                "expected `<sequence>-<year>` form, got `{raw}`"
            )));
        }
        if !parts[0].chars().all(|c| c.is_ascii_digit()) {
            return Err(TedError::InvalidNoticeId(format!(
                "sequence must be numeric: `{}`",
                parts[0]
            )));
        }
        if parts[1].len() != 4 || !parts[1].chars().all(|c| c.is_ascii_digit()) {
            return Err(TedError::InvalidNoticeId(format!(
                "year must be 4-digit numeric: `{}`",
                parts[1]
            )));
        }
        Ok(Self(raw.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Procurement notice type. TED publishes ~20 distinct notice types
/// per the EU eForms taxonomy; the port keeps the closed-enum set
/// minimal (the operationally distinct ones) plus an `Other` slot
/// for the rest. Expand when an app pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementType {
    /// Prior information notice — pre-tender signal of upcoming work.
    PriorInformation,
    /// Contract notice — the formal call for tenders.
    ContractNotice,
    /// Contract award notice — the contract has been awarded.
    ContractAwardNotice,
    /// Modification of an existing contract.
    Modification,
    /// Anything else (corrigenda, voluntary ex-ante transparency,
    /// design contests, etc.).
    Other,
}

/// One TED procurement notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementNotice {
    pub notice_id: TedNoticeId,
    pub contracting_authority: String,
    pub title: String,
    /// ISO 3166-1 alpha-2 country of the contracting authority.
    pub country: String,
    pub procurement_type: ProcurementType,
    /// ISO 8601 datetime of the submission deadline (when applicable).
    /// Award notices and modifications do not carry a deadline.
    pub deadline: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_id_rejects_empty() {
        // Intent: empty IDs at the boundary mean a malformed upstream
        // record; refuse loudly.
        assert!(TedNoticeId::parse("").is_err());
    }

    #[test]
    fn notice_id_requires_sequence_year_form() {
        // Intent: TED's canonical form is `<sequence>-<year>`. The
        // port rejects free-form strings so a downstream consumer
        // joining on this ID against an external TED dataset doesn't
        // miss matches due to format drift.
        assert!(TedNoticeId::parse("123456").is_err());
        assert!(TedNoticeId::parse("abc-2026").is_err());
        assert!(TedNoticeId::parse("123456-26").is_err());
        assert!(TedNoticeId::parse("123456-2026").is_ok());
    }

    #[test]
    fn notice_serde_round_trips() {
        let notice = ProcurementNotice {
            notice_id: TedNoticeId::parse("123456-2026").unwrap(),
            contracting_authority: "Stockholm Stad".into(),
            title: "Procurement of stub services".into(),
            country: "SE".into(),
            procurement_type: ProcurementType::ContractNotice,
            deadline: Some("2026-06-01T12:00:00Z".into()),
        };
        let json = serde_json::to_string(&notice).unwrap();
        let back: ProcurementNotice = serde_json::from_str(&json).unwrap();
        assert_eq!(back, notice);
    }
}
