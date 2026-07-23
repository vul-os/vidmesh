//! Subscription filters (spec 006 §3).
//!
//! A filter is a CBOR map with UTF-8 text keys (following the body
//! convention of spec 003 §2, since the filter is not part of the
//! signed envelope). All present conditions AND together; each list
//! condition matches any element (OR within the list). An empty filter
//! matches everything.
//!
//! Filters are parsed from the generic [`crate::frames::Value`] tree
//! produced by the frame codec rather than from a second CBOR library,
//! per the module's design brief.

use crate::frames::Value;

/// A parsed subscription filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Match records whose `kind` is in this list.
    pub kinds: Option<Vec<u64>>,
    /// Match records whose `author.identity_id` is in this list.
    pub authors: Option<Vec<[u8; 32]>>,
    /// Match records with at least one ref hash in this list.
    pub refs: Option<Vec<[u8; 32]>>,
    /// Match records whose id is in this list.
    pub ids: Option<Vec<[u8; 32]>>,
    /// Match records with relay-local `seq` strictly greater than this.
    pub since: Option<u64>,
    /// Stored-phase cap: at most this many stored records are sent,
    /// most-recent-first, before `EOSE`. Not a per-record match
    /// condition; consulted by the store query, not [`Filter::matches`].
    pub limit: Option<u64>,
}

/// Everything that can go wrong turning a decoded frame [`Value`] into
/// a [`Filter`]. Never panics: malformed filters are rejected frame-side
/// (typically as `CLOSED`), never crash the relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// The filter value was not a CBOR map.
    NotAMap,
    /// `kinds` was present but not an array of unsigned integers.
    BadKinds,
    /// `authors` was present but not an array of 32-byte strings.
    BadAuthors,
    /// `refs` was present but not an array of 32-byte strings.
    BadRefs,
    /// `ids` was present but not an array of 32-byte strings.
    BadIds,
    /// `since` was present but not an unsigned integer.
    BadSince,
    /// `limit` was present but not an unsigned integer.
    BadLimit,
}

impl Filter {
    /// Parse a filter from a decoded CBOR map. Unknown keys are ignored
    /// (forward-compatible, matching the body-extension convention).
    pub(crate) fn from_value(v: &Value) -> Result<Filter, FilterError> {
        let pairs = v.as_map().ok_or(FilterError::NotAMap)?;
        let mut f = Filter::default();
        for (key, val) in pairs {
            match key.as_text() {
                Some("kinds") => f.kinds = Some(parse_uint_list(val).ok_or(FilterError::BadKinds)?),
                Some("authors") => {
                    f.authors = Some(parse_bytes32_list(val).ok_or(FilterError::BadAuthors)?)
                }
                Some("refs") => f.refs = Some(parse_bytes32_list(val).ok_or(FilterError::BadRefs)?),
                Some("ids") => f.ids = Some(parse_bytes32_list(val).ok_or(FilterError::BadIds)?),
                Some("since") => f.since = Some(val.as_uint().ok_or(FilterError::BadSince)?),
                Some("limit") => f.limit = Some(val.as_uint().ok_or(FilterError::BadLimit)?),
                _ => { /* unknown key: ignored */ }
            }
        }
        Ok(f)
    }

    /// Encode this filter back into a frame [`Value`] (used by gossip's
    /// outbound `REQ`, and available to tests / conformance tooling).
    pub(crate) fn to_value(&self) -> Value {
        let mut pairs = Vec::new();
        if let Some(kinds) = &self.kinds {
            pairs.push((
                Value::Text("kinds".to_string()),
                Value::Array(kinds.iter().map(|k| Value::Uint(*k)).collect()),
            ));
        }
        if let Some(authors) = &self.authors {
            pairs.push((
                Value::Text("authors".to_string()),
                Value::Array(authors.iter().map(|a| Value::Bytes(a.to_vec())).collect()),
            ));
        }
        if let Some(refs) = &self.refs {
            pairs.push((
                Value::Text("refs".to_string()),
                Value::Array(refs.iter().map(|r| Value::Bytes(r.to_vec())).collect()),
            ));
        }
        if let Some(ids) = &self.ids {
            pairs.push((
                Value::Text("ids".to_string()),
                Value::Array(ids.iter().map(|i| Value::Bytes(i.to_vec())).collect()),
            ));
        }
        if let Some(since) = self.since {
            pairs.push((Value::Text("since".to_string()), Value::Uint(since)));
        }
        if let Some(limit) = self.limit {
            pairs.push((Value::Text("limit".to_string()), Value::Uint(limit)));
        }
        Value::Map(pairs)
    }

    /// Whether a record with the given properties matches this filter.
    /// `seq` is the record's relay-local receipt sequence (spec §2).
    ///
    /// `limit` is deliberately not consulted here: it caps the
    /// stored-phase backfill (see [`crate::store::Store::query`]), it
    /// is not a per-record predicate.
    pub fn matches(
        &self,
        kind: u64,
        author: &[u8; 32],
        id: &[u8; 32],
        ref_hashes: &[[u8; 32]],
        seq: u64,
    ) -> bool {
        if let Some(kinds) = &self.kinds {
            if !kinds.contains(&kind) {
                return false;
            }
        }
        if let Some(authors) = &self.authors {
            if !authors.iter().any(|a| a == author) {
                return false;
            }
        }
        if let Some(ids) = &self.ids {
            if !ids.iter().any(|i| i == id) {
                return false;
            }
        }
        if let Some(refs) = &self.refs {
            if !refs.iter().any(|r| ref_hashes.contains(r)) {
                return false;
            }
        }
        if let Some(since) = self.since {
            if seq <= since {
                return false;
            }
        }
        true
    }
}

fn parse_uint_list(v: &Value) -> Option<Vec<u64>> {
    v.as_array()?.iter().map(Value::as_uint).collect()
}

fn parse_bytes32_list(v: &Value) -> Option<Vec<[u8; 32]>> {
    v.as_array()?.iter().map(Value::as_bytes32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filter_matches_everything() {
        let f = Filter::default();
        assert!(f.matches(1, &[0u8; 32], &[0u8; 32], &[], 1));
    }

    #[test]
    fn kinds_filters() {
        let f = Filter {
            kinds: Some(vec![2, 3]),
            ..Default::default()
        };
        assert!(f.matches(2, &[0; 32], &[0; 32], &[], 1));
        assert!(!f.matches(4, &[0; 32], &[0; 32], &[], 1));
    }

    #[test]
    fn authors_filters() {
        let a = [1u8; 32];
        let f = Filter {
            authors: Some(vec![a]),
            ..Default::default()
        };
        assert!(f.matches(1, &a, &[0; 32], &[], 1));
        assert!(!f.matches(1, &[2u8; 32], &[0; 32], &[], 1));
    }

    #[test]
    fn ids_filters() {
        let id = [9u8; 32];
        let f = Filter {
            ids: Some(vec![id]),
            ..Default::default()
        };
        assert!(f.matches(1, &[0; 32], &id, &[], 1));
        assert!(!f.matches(1, &[0; 32], &[8u8; 32], &[], 1));
    }

    #[test]
    fn refs_filters_any_overlap() {
        let target = [5u8; 32];
        let f = Filter {
            refs: Some(vec![target]),
            ..Default::default()
        };
        assert!(f.matches(1, &[0; 32], &[0; 32], &[[1u8; 32], target], 1));
        assert!(!f.matches(1, &[0; 32], &[0; 32], &[[1u8; 32]], 1));
    }

    #[test]
    fn since_is_strictly_greater() {
        let f = Filter {
            since: Some(10),
            ..Default::default()
        };
        assert!(!f.matches(1, &[0; 32], &[0; 32], &[], 10));
        assert!(f.matches(1, &[0; 32], &[0; 32], &[], 11));
    }

    #[test]
    fn value_round_trip() {
        let f = Filter {
            kinds: Some(vec![1, 2]),
            authors: Some(vec![[3u8; 32]]),
            refs: Some(vec![[4u8; 32]]),
            ids: Some(vec![[5u8; 32]]),
            since: Some(6),
            limit: Some(7),
        };
        let value = f.to_value();
        let parsed = Filter::from_value(&value).unwrap();
        assert_eq!(parsed, f);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let value = Value::Map(vec![(Value::Text("mystery".to_string()), Value::Uint(1))]);
        let f = Filter::from_value(&value).unwrap();
        assert_eq!(f, Filter::default());
    }

    #[test]
    fn wrong_type_is_rejected_not_panicked() {
        let value = Value::Map(vec![(
            Value::Text("since".to_string()),
            Value::Text("not a uint".to_string()),
        )]);
        assert_eq!(Filter::from_value(&value), Err(FilterError::BadSince));
    }

    #[test]
    fn non_map_is_rejected() {
        assert_eq!(
            Filter::from_value(&Value::Array(vec![])),
            Err(FilterError::NotAMap)
        );
    }
}
