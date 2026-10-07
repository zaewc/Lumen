//! BLAKE3 content digests with a strict `b3:<hex>` text form.
//!
//! Content addressing makes evidence, bundles and plans verifiable: a decision or
//! a confirmation names exactly the bytes it was about (ADR-0013, ADR-0023).

/// Defines a 32-byte BLAKE3 digest newtype whose text form is `b3:` followed by
/// 64 lowercase hex digits (strict: no uppercase, no other prefix).
macro_rules! b3_digest {
    ($(#[$meta:meta])* $name:ident, $error:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        #[doc = concat!("Error returned for a malformed ", $what, ".")]
        #[derive(Debug, Clone, PartialEq, Eq, ::thiserror::Error)]
        #[error("{} must be 'b3:' followed by 64 lowercase hex digits", $what)]
        pub struct $error;

        impl $name {
            /// Wraps a raw 32-byte BLAKE3 digest.
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// The raw 32-byte digest.
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str("b3:")?;
                self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}({self})", stringify!($name))
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let hex = s.strip_prefix("b3:").ok_or($error)?;
                if hex.len() != 64 {
                    return Err($error);
                }
                let nibble = |b: u8| match b {
                    b'0'..=b'9' => Ok(b - b'0'),
                    b'a'..=b'f' => Ok(b - b'a' + 10),
                    _ => Err($error),
                };
                let mut out = [0u8; 32];
                for (byte, &[hi, lo]) in out.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
                    *byte = (nibble(hi)? << 4) | nibble(lo)?;
                }
                Ok(Self(out))
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <::std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                text.parse().map_err(::serde::de::Error::custom)
            }
        }
    };
}

b3_digest! {
    /// Digest of a whole evidence bundle (the evidence a decision used). Recorded
    /// with every policy decision and Jev trace so the decision can be replayed
    /// (ADR-0013, ADR-0014). Computed as a Merkle root by the evidence graph.
    EvidenceHash, EvidenceHashError, "evidence hash"
}

b3_digest! {
    /// Digest of a cleanup plan's canonical form. A user confirmation names this
    /// hash, so the plan that executes is exactly the plan that was shown.
    PlanHash, PlanHashError, "plan hash"
}
