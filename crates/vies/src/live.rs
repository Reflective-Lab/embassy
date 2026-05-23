// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live VIES SOAP transport.
//!
//! Hits the public EU VIES SOAP endpoint at
//! `https://ec.europa.eu/taxation_customs/vies/services/checkVatService`.
//! VIES is a free public service for validating EU VAT numbers
//! against member-state databases; no auth, no key.
//!
//! Gated behind the `live` feature. Default-features builds keep the
//! typed domain + [`StubViesProvider`] only.
//!
//! ## SOAP shape
//!
//! Request:
//! ```xml
//! <soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/"
//!                   xmlns:tns="urn:ec.europa.eu:taxud:vies:services:checkVat:types">
//!   <soapenv:Body>
//!     <tns:checkVat>
//!       <tns:countryCode>SE</tns:countryCode>
//!       <tns:vatNumber>556036080501</tns:vatNumber>
//!     </tns:checkVat>
//!   </soapenv:Body>
//! </soapenv:Envelope>
//! ```
//!
//! Response (success):
//! ```xml
//! <env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/">
//!   <env:Body>
//!     <ns2:checkVatResponse xmlns:ns2="urn:...">
//!       <ns2:countryCode>SE</ns2:countryCode>
//!       <ns2:vatNumber>556036080501</ns2:vatNumber>
//!       <ns2:requestDate>2026-05-23+01:00</ns2:requestDate>
//!       <ns2:valid>true</ns2:valid>
//!       <ns2:name>VOLVO AKTIEBOLAG</ns2:name>
//!       <ns2:address>...</ns2:address>
//!     </ns2:checkVatResponse>
//!   </env:Body>
//! </env:Envelope>
//! ```
//!
//! Response (member-state DB offline) returns a SOAP `<env:Fault>` or
//! a `MS_UNAVAILABLE` faultstring — distinct from "checked and not
//! valid". The live provider maps that to
//! `MemberStateStatus::Unavailable`.

use std::time::Instant;

use async_trait::async_trait;
use chrono::SecondsFormat;
use manifold::xml;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use thiserror::Error;

use crate::error::ViesError;
use crate::provider::{ViesProvider, ViesRequest, ViesResponse};
use crate::types::{MemberStateStatus, VatNumber, VatValidation};
use crate::{Observation, content_hash};

pub const DEFAULT_SOAP_URL: &str =
    "https://ec.europa.eu/taxation_customs/vies/services/checkVatService";

pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";
pub const DEFAULT_MAX_BYTES: usize = 256 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub soap_url: String,
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            soap_url: DEFAULT_SOAP_URL.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Error)]
pub enum LiveError {
    #[error("fetch failed: {0}")]
    Fetch(String),
    #[error("HTTP {status}")]
    Http { status: u16 },
    #[error("SOAP fault: {0}")]
    SoapFault(String),
    #[error("response missing required field: {0}")]
    MissingField(&'static str),
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("xml parse failed: {0}")]
    Xml(String),
}

#[derive(Debug, Clone, Default)]
pub struct LiveViesProvider {
    options: LiveFetchOptions,
}

impl LiveViesProvider {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_options(options: LiveFetchOptions) -> Self {
        Self { options }
    }
}

#[async_trait]
impl ViesProvider for LiveViesProvider {
    fn name(&self) -> &'static str {
        "live_vies"
    }

    async fn validate(
        &self,
        request: &ViesRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<ViesResponse, ViesError> {
        let started = Instant::now();
        let ViesRequest::Validate { vat_number } = request;
        let envelope = build_check_vat_envelope(vat_number);
        let body = post_soap(&self.options, &envelope)
            .await
            .map_err(live_error)?;
        let validation = parse_check_vat_response(&body, vat_number).map_err(live_error)?;

        let request_json = serde_json::to_string(request)
            .map_err(|e| ViesError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let observation = Observation {
            observation_id: format!("obs:vies-live:{request_hash}"),
            request_hash,
            vendor: self.name().to_string(),
            model: "vies-soap-v1".to_string(),
            latency_ms,
            cost_estimate: None,
            tokens: None,
            content: validation,
            raw_response: Some(body),
        };

        Ok(ViesResponse {
            records: vec![observation],
        })
    }
}

fn live_error(err: LiveError) -> ViesError {
    ViesError::Transport(err.to_string())
}

/// Build the SOAP envelope for a `checkVat` request. VIES is
/// permissive about the soap envelope prefix and the `tns:` prefix on
/// the body element; we use the form documented in the WSDL.
fn build_check_vat_envelope(vat: &VatNumber) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/"
                  xmlns:tns="urn:ec.europa.eu:taxud:vies:services:checkVat:types">
  <soapenv:Header/>
  <soapenv:Body>
    <tns:checkVat>
      <tns:countryCode>{cc}</tns:countryCode>
      <tns:vatNumber>{nat}</tns:vatNumber>
    </tns:checkVat>
  </soapenv:Body>
</soapenv:Envelope>"#,
        cc = xml_escape(vat.country.as_str()),
        nat = xml_escape(&vat.national),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

async fn post_soap(opts: &LiveFetchOptions, envelope: &str) -> Result<String, LiveError> {
    let opts = opts.clone();
    let envelope = envelope.to_string();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new()
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_user_agent(&opts.user_agent);
        let request = WebFetchRequest::new(&opts.soap_url)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_max_bytes(opts.max_bytes)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_timeout_ms(opts.timeout_ms)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_header("Content-Type", "text/xml; charset=utf-8")
            .with_header("SOAPAction", "")
            .with_body(envelope);
        let response = provider
            .fetch(&request)
            .map_err(|e| LiveError::Fetch(e.to_string()))?;
        // VIES returns 200 even for SOAP faults; the fault is in the
        // body. Surface non-200 separately so transport-level failures
        // (504 Bad Gateway, etc.) don't get parsed as SOAP responses.
        if response.status >= 400 {
            return Err(LiveError::Http {
                status: response.status,
            });
        }
        Ok(response.body)
    })
    .await?
}

/// Parse a VIES SOAP response into a typed [`VatValidation`].
///
/// Handles three response shapes:
/// 1. Success: `<checkVatResponse>` with `<valid>`, `<name>`, `<address>`.
///    Maps to `member_state_status = Available`, `valid` per the field.
/// 2. SOAP fault with `MS_UNAVAILABLE`: maps to
///    `member_state_status = Unavailable`, `valid = false`. Re-trying
///    later may succeed.
/// 3. Other SOAP faults (`INVALID_INPUT`, `SERVICE_UNAVAILABLE`, etc.):
///    surfaced as `LiveError::SoapFault` — the caller decides whether
///    to retry, escalate, or fail.
fn parse_check_vat_response(
    body: &str,
    request_vat: &VatNumber,
) -> Result<VatValidation, LiveError> {
    if let Some(faultstring) =
        xml::extract_first_text(body, "faultstring").map_err(|e| LiveError::Xml(e.to_string()))?
    {
        let trimmed = faultstring.trim();
        if trimmed.eq_ignore_ascii_case("MS_UNAVAILABLE") {
            return Ok(VatValidation {
                vat_number: request_vat.clone(),
                valid: false,
                member_state_status: MemberStateStatus::Unavailable,
                consultation_number: None,
                registered_name: None,
                registered_address: None,
                validated_at: now_rfc3339(),
            });
        }
        return Err(LiveError::SoapFault(trimmed.to_string()));
    }

    let valid_raw = xml::extract_first_text(body, "valid")
        .map_err(|e| LiveError::Xml(e.to_string()))?
        .ok_or(LiveError::MissingField("valid"))?;
    let valid = matches!(valid_raw.trim().to_ascii_lowercase().as_str(), "true" | "1");

    let consultation_number = xml::extract_first_text(body, "requestIdentifier")
        .map_err(|e| LiveError::Xml(e.to_string()))?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let registered_name = xml::extract_first_text(body, "name")
        .map_err(|e| LiveError::Xml(e.to_string()))?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "---");

    let registered_address = xml::extract_first_text(body, "address")
        .map_err(|e| LiveError::Xml(e.to_string()))?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "---");

    Ok(VatValidation {
        vat_number: request_vat.clone(),
        valid,
        member_state_status: MemberStateStatus::Available,
        consultation_number,
        registered_name,
        registered_address,
        validated_at: now_rfc3339(),
    })
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canned successful VIES response. Real responses have a varying
    /// namespace prefix on the inner elements; the parser uses
    /// local-name matching so this prefix doesn't matter.
    const SUCCESS_VALID: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/">
  <env:Body>
    <ns2:checkVatResponse xmlns:ns2="urn:ec.europa.eu:taxud:vies:services:checkVat:types">
      <ns2:countryCode>SE</ns2:countryCode>
      <ns2:vatNumber>556036080501</ns2:vatNumber>
      <ns2:requestDate>2026-05-23+01:00</ns2:requestDate>
      <ns2:valid>true</ns2:valid>
      <ns2:requestIdentifier>WAPIAAAA0aGZ12345</ns2:requestIdentifier>
      <ns2:name>VOLVO AKTIEBOLAG</ns2:name>
      <ns2:address>VOLVO LUNDBYHOLM 405 08 GOTEBORG</ns2:address>
    </ns2:checkVatResponse>
  </env:Body>
</env:Envelope>"#;

    const SUCCESS_INVALID: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/">
  <env:Body>
    <ns2:checkVatResponse xmlns:ns2="urn:ec.europa.eu:taxud:vies:services:checkVat:types">
      <ns2:countryCode>SE</ns2:countryCode>
      <ns2:vatNumber>0000000000</ns2:vatNumber>
      <ns2:requestDate>2026-05-23+01:00</ns2:requestDate>
      <ns2:valid>false</ns2:valid>
      <ns2:requestIdentifier>WAPIAAAA0aGZ99999</ns2:requestIdentifier>
      <ns2:name>---</ns2:name>
      <ns2:address>---</ns2:address>
    </ns2:checkVatResponse>
  </env:Body>
</env:Envelope>"#;

    const MS_UNAVAILABLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/">
  <env:Body>
    <env:Fault>
      <faultcode>env:Server</faultcode>
      <faultstring>MS_UNAVAILABLE</faultstring>
    </env:Fault>
  </env:Body>
</env:Envelope>"#;

    const INVALID_INPUT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<env:Envelope xmlns:env="http://schemas.xmlsoap.org/soap/envelope/">
  <env:Body>
    <env:Fault>
      <faultcode>env:Server</faultcode>
      <faultstring>INVALID_INPUT</faultstring>
    </env:Fault>
  </env:Body>
</env:Envelope>"#;

    fn volvo() -> VatNumber {
        VatNumber::parse("SE556036080501").unwrap()
    }

    #[test]
    fn parse_success_valid_returns_typed_validation() {
        // Intent: a successful registered-VAT response must surface
        // the name + consultation number. Missing any of those fields
        // breaks the audit-trail contract downstream code relies on
        // ("we asked, here is the proof identifier and the registered
        // name at that moment").
        let v = parse_check_vat_response(SUCCESS_VALID, &volvo()).unwrap();
        assert!(v.valid);
        assert_eq!(v.member_state_status, MemberStateStatus::Available);
        assert_eq!(v.consultation_number.as_deref(), Some("WAPIAAAA0aGZ12345"));
        assert_eq!(v.registered_name.as_deref(), Some("VOLVO AKTIEBOLAG"));
        assert_eq!(
            v.registered_address.as_deref(),
            Some("VOLVO LUNDBYHOLM 405 08 GOTEBORG")
        );
    }

    #[test]
    fn parse_success_invalid_drops_dashes_for_name_and_address() {
        // Intent: VIES returns `"---"` in name/address when the member
        // state declines to disclose for a `valid = false` answer. We
        // must surface those as `None`, not as the literal `"---"`.
        // Otherwise downstream rendering shows three dashes as if they
        // were the registered name.
        let v = parse_check_vat_response(SUCCESS_INVALID, &volvo()).unwrap();
        assert!(!v.valid);
        assert_eq!(v.member_state_status, MemberStateStatus::Available);
        assert!(v.registered_name.is_none());
        assert!(v.registered_address.is_none());
    }

    #[test]
    fn ms_unavailable_maps_to_unavailable_status_not_invalid() {
        // Intent: an offline member-state DB is a distinct signal from
        // "checked and not valid". Re-trying the call later may
        // succeed. Conflating these two would let stale-cached
        // "invalid" pollute downstream decisions when the truth is
        // "unknown right now".
        let v = parse_check_vat_response(MS_UNAVAILABLE, &volvo()).unwrap();
        assert_eq!(v.member_state_status, MemberStateStatus::Unavailable);
        assert!(!v.valid);
        assert!(v.consultation_number.is_none());
    }

    #[test]
    fn invalid_input_surfaces_as_soap_fault_error() {
        // Intent: malformed input should error loudly. Silently
        // returning `valid = false` would be a confusion of two
        // different signals.
        let result = parse_check_vat_response(INVALID_INPUT, &volvo());
        assert!(matches!(result, Err(LiveError::SoapFault(_))));
    }

    #[test]
    fn envelope_escapes_xml_specials() {
        // Intent: an attacker-controlled VAT input must not break the
        // envelope. The parser already rejects non-alphanumeric input
        // upstream (see VatNumber::parse), but defense-in-depth here
        // costs nothing.
        let escaped = xml_escape(r#"abc<>"&'"#);
        assert_eq!(escaped, "abc&lt;&gt;&quot;&amp;&apos;");
    }

    /// Live network test — disabled by default. Run with:
    ///     VIES_LIVE_TEST=1 cargo test -p converge-embassy-vies \
    ///         --features live -- --ignored live_validate
    #[tokio::test]
    #[ignore = "live SOAP call to ec.europa.eu; opt-in only"]
    async fn live_validate() {
        if std::env::var("VIES_LIVE_TEST").is_err() {
            eprintln!("Set VIES_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveViesProvider::new();
        let req = ViesRequest::Validate {
            vat_number: volvo(),
        };
        let resp = provider
            .validate(&req, &embassy_pack::CallContext::default())
            .await
            .expect("VIES validate");
        assert_eq!(resp.records.len(), 1);
        // Volvo's VAT number is a real registered SE VAT — if VIES is
        // reachable and the SE database is up, this returns valid=true.
        // We don't assert valid==true to keep the test robust to
        // MS_UNAVAILABLE responses.
        let v = &resp.records[0].content;
        assert_eq!(v.vat_number.country.as_str(), "SE");
    }
}
