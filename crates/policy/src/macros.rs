//! Declarative helper for closed, string-identified enums.

/// Define a closed enum whose variants have stable string identifiers.
///
/// Generates `ALL`, `as_str`, `Display`, `FromStr` (unknown strings are
/// [`crate::PolicyError::UnknownValue`]), and string-based serde impls, so an
/// identifier that does not parse can never become a value.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident ($kind:literal) {
            $( $(#[$vmeta:meta])* $variant:ident => $id:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stable identifier.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $id),+
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::PolicyError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($id => Ok($name::$variant),)+
                    _ => Err($crate::PolicyError::unknown($kind, value)),
                }
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <::std::string::String as ::serde::Deserialize>::deserialize(deserializer)?;
                value.parse().map_err(::serde::de::Error::custom)
            }
        }
    };
}

pub(crate) use string_enum;
