/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! DNS wire-format (RFC 1035) message building and parsing, backed by
//! `hickory-proto` for battle-tested encoding and compression-pointer handling.

use hickory_proto::op::{Message, Query, ResponseCode};
use hickory_proto::rr::{Name, RData, RecordType};

use crate::error::BrowNetError;

/// Build an A or AAAA query for `host` in DNS wire format.
///
/// The resolver issues A and AAAA as separate messages (one qtype per query,
/// per RFC 1035) and merges the answers; see [`build_queries`].
pub fn build_query_for(host: &str, record_type: RecordType) -> Result<Vec<u8>, BrowNetError> {
    let name = Name::from_utf8(host)
        .map_err(|e| BrowNetError::Doh(format!("invalid host name '{host}': {e}")))?;

    let mut message = Message::query();
    message.metadata.recursion_desired = true;
    message.add_query(Query::query(name, record_type));

    message
        .to_vec()
        .map_err(|e| BrowNetError::Doh(format!("failed to encode DNS query: {e}")))
}

/// Build both A and AAAA queries for `host`.
pub fn build_queries(host: &str) -> Result<(Vec<u8>, Vec<u8>), BrowNetError> {
    Ok((
        build_query_for(host, RecordType::A)?,
        build_query_for(host, RecordType::AAAA)?,
    ))
}

/// Convenience single-query builder (A record) used by simple paths.
pub fn build_query(host: &str) -> Result<Vec<u8>, BrowNetError> {
    build_query_for(host, RecordType::A)
}

/// Parse a DNS response message and extract A/AAAA addresses with their TTL.
pub fn parse_response(bytes: &[u8], host: &str) -> Result<super::ResolvedAddrs, BrowNetError> {
    let message = Message::from_vec(bytes)
        .map_err(|e| BrowNetError::Doh(format!("malformed DNS response: {e}")))?;

    if message.metadata.response_code != ResponseCode::NoError {
        return Err(BrowNetError::Doh(format!(
            "DNS response code: {:?}",
            message.metadata.response_code
        )));
    }

    // Guard against spoofed answers: the question must be for the host we
    // asked about (RFC 8484 §5.1 — DoH servers echo the question section).
    // Compare hickory `Name`s; the expected name is made explicitly absolute
    // (`host.`) so it matches the wire-parsed question name.
    let expected = Name::from_utf8(&format!("{host}."))
        .ok()
        .map(|n| n.to_lowercase());
    if let (Some(expected), Some(query)) = (expected, message.queries.first()) {
        if query.name().to_lowercase() != expected {
            return Err(BrowNetError::Doh(format!(
                "DNS response question mismatch: asked for {host}, got {}",
                query.name()
            )));
        }
    }

    let mut addrs = Vec::new();
    let mut min_ttl = u32::MAX;
    for answer in &message.answers {
        min_ttl = min_ttl.min(answer.ttl);
        match &answer.data {
            RData::A(a) => addrs.push(std::net::IpAddr::V4(a.0)),
            RData::AAAA(aaaa) => addrs.push(std::net::IpAddr::V6(aaaa.0)),
            _ => {},
        }
    }

    if addrs.is_empty() {
        return Err(BrowNetError::Doh(format!("no A/AAAA records for {host}")));
    }

    // Clamp TTL to [10s, 3600s] to bound both re-query traffic and staleness.
    let ttl = min_ttl.clamp(10, 3600);
    Ok(super::ResolvedAddrs {
        addrs,
        ttl: std::time::Duration::from_secs(u64::from(ttl)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::op::{MessageType, OpCode};
    use hickory_proto::rr::rdata::a::A;
    use hickory_proto::rr::Record;

    #[test]
    fn builds_valid_query_for_valid_host() {
        let bytes = build_query("example.com").unwrap();
        let parsed = Message::from_vec(&bytes).unwrap();
        assert_eq!(parsed.metadata.message_type, MessageType::Query);
        assert_eq!(parsed.queries.len(), 1);
        assert_eq!(parsed.queries[0].query_type(), RecordType::A);
        assert!(parsed.metadata.recursion_desired);
    }

    #[test]
    fn rejects_invalid_host() {
        assert!(build_query("not a host..").is_err());
    }

    #[test]
    fn builds_and_parses_a_response_roundtrip() {
        // Server side: construct a real response with an A record.
        let query_bytes = build_query("example.test").unwrap();
        let query = Message::from_vec(&query_bytes).unwrap();

        let mut response = Message::response(query.metadata.id, OpCode::Query);
        for q in &query.queries {
            response.add_query(q.clone());
        }
        response.add_answer(Record::from_rdata(
            Name::from_utf8("example.test").unwrap(),
            120,
            RData::A(A("93.184.216.34".parse().unwrap())),
        ));
        let response_bytes = response.to_vec().unwrap();

        let resolved = parse_response(&response_bytes, "example.test").unwrap();
        assert_eq!(
            resolved.addrs,
            vec!["93.184.216.34".parse::<std::net::IpAddr>().unwrap()]
        );
        assert_eq!(resolved.ttl, std::time::Duration::from_secs(120));
    }

    #[test]
    fn rejects_name_mismatch() {
        let query_bytes = build_query("example.test").unwrap();
        let query = Message::from_vec(&query_bytes).unwrap();
        let mut response = Message::response(query.metadata.id, OpCode::Query);
        // A tampered server answers for a DIFFERENT name than asked.
        response.add_query(Query::query(
            Name::from_utf8("evil.test.").unwrap(),
            RecordType::A,
        ));
        response.add_answer(Record::from_rdata(
            Name::from_utf8("evil.test.").unwrap(),
            120,
            RData::A(A("6.6.6.6".parse().unwrap())),
        ));
        let response_bytes = response.to_vec().unwrap();
        assert!(parse_response(&response_bytes, "example.test").is_err());
    }

    #[test]
    fn rejects_nx_domain() {
        let query_bytes = build_query("example.test").unwrap();
        let query = Message::from_vec(&query_bytes).unwrap();
        let mut response = Message::response(query.metadata.id, OpCode::Query);
        response.metadata.response_code = ResponseCode::NXDomain;
        let response_bytes = response.to_vec().unwrap();
        assert!(parse_response(&response_bytes, "example.test").is_err());
    }
}
