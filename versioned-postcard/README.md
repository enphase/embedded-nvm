# versioned-postcard

Versioned, migratable postcard records.
Each version is defined with a reference to (and migration function from) the prior version struct.
Version stored in a tag as a leading varint.
Support for appended new fields migration within the same version tag using Option fields.

`no_std`, no allocator.


## A single version

```rust
// uses versioned-postcard with feature "max-size", which also pulls in postcard "experimental-derive"
use postcard::experimental::max_size::MaxSize;
use versioned_postcard::{Base, Version};

// your custom settings structure, seen by your app
#[derive(Clone, Copy, Default, PartialEq, Debug, serde::Serialize, serde::Deserialize, MaxSize)]
struct Settings {
    a: u16,
}

impl Version for Settings {
    const TAG: u16 = 1;  // version tag, must start at 1
    type Wire = Settings;

    // Base marks this as the first version with no different-version-tag migration
    // optionally, this could specify another Version and define a migration function
    type Prev = Base;
    fn from_prev(p: Base) -> Settings { match p {} }
}

// RECORD_MAX is the largest serialized record across the whole version chain
let mut buf = [0u8; 1 + <Settings as Version>::RECORD_MAX];
let n = Settings { a: 7 }.serialize_into(&mut buf).unwrap();
assert_eq!(
    Settings::deserialize_from::<{ 1 + <Settings as Version>::RECORD_MAX }>(&buf[..n]).unwrap(),
    Settings { a: 7 },
);
```

## Appending a field without a new version tag

Define a separate `Wire` struct whose appended fields are `Option` with converters in both directions.
Records written by the older firmware decode with `None` for the appended field, and normalization substitutes the default.

```rust
use postcard::experimental::max_size::MaxSize;
use versioned_postcard::{Base, Version};

// the app-facing settings structure is clean and Option-free
#[derive(Clone, Copy, PartialEq, Debug)]
struct Settings {
    a: u16,
    b: i16,  // appended field from the prior version above
}
impl Default for Settings {
    fn default() -> Self {
        Settings { a: 0, b: -99 }
    }
}

// defines how to serialize and deserialize the settings structure
// here, it handles Option (un)wrapping for compatibility
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, MaxSize)]
struct SettingsWire {
    a: u16,
    b: Option<i16>,  // this actually handles the (de)serialization
}
impl From<SettingsWire> for Settings {
    fn from(w: SettingsWire) -> Settings {
        Settings {
            a: w.a,
            b: w.b.unwrap_or(Settings::default().b),  // creates a default for appended field
        }
    }
}
impl From<Settings> for SettingsWire {
    fn from(s: Settings) -> SettingsWire {
        SettingsWire {
            a: s.a,
            b: Some(s.b),  // packs appended field into Option on wire
        }
    }
}
```
