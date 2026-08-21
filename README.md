# embedded-nvm

A collection of crates for embedded nonvolatile memory (NVM) operations with a focus on simplifying management of device settings.


## Crates

- `versioned-postcard`: Versioned, migratable postcard records for no_std. Each version is defined with a reference to (and migration function from) the prior version struct. Version stored in a tag as a leading varint. Support for appended new fields migration within the same version tag using Option fields.
- `embedded-nvm-settings`: `NvmSettings` object that presents a in-memory cached settings with modify-update, deferred async commit, agnostic storage backend, and agnostic serialization backend. Built-in adapters to work with versioned-postcard and vanilla postcard.


## `versioned-postcard` example

```rust
// uses versioned-postcard with feature "max-size", which also pulls in postcard "experimental-derive"

use postcard::experimental::max_size::MaxSize;
use versioned_postcard::{Base, Version};

// your custom settings structure, seen by your app
#[derive(Clone, Copy, PartialEq, Debug)]
struct Settings { 
    a: u16, 
    b: i16 
}
impl Default for Settings { 
    fn default() -> Self { 
        Settings { 
            a: 0, 
            b: -99 
        } 
    } 
}

// a wire format for your settings structure, defines the serialization structure
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, MaxSize)]
struct SettingsWire { 
    a: u16, 
    b: Option<i16>  // an appended field for within-version-tag migration
}

// defines conversion to and from the wire format
impl From<SettingsWire> for Settings { 
    fn from(w: SettingsWire) -> Settings { 
        Settings {
            a: w.a, 
            b: w.b.unwrap_or(Settings::default().b)  // creates a default for appended field
        } 
    } 
}
impl From<Settings> for SettingsWire { 
    fn from(s: Settings) -> SettingsWire {
        SettingsWire {
            a: s.a, 
            b: Some(s.b)  // packs appended field into Option on wire
        } 
    } 
}

impl Version for Settings {
    const TAG: u16 = 1;  // version tag, must start at 1
    type Wire = SettingsWire;

    // Base marks this as the first version with no different-version-tag migration
    // optionally, this could specify another Version and define a migration function
    type Prev = Base;
    fn from_prev(p: Base) -> Settings { match p {} }
}
```

## `embedded-nvm-settings` example

```rust
// flash HAL adapteres, here for the STM32
type AsyncFlash = embassy_embedded_hal::adapter::BlockingAsync<embassy_stm32::flash::Flash<'static, embassy_stm32::flash::Blocking>>;
let flash = BlockingAsync::new(Flash::new_blocking(p.FLASH));

// convenience type alias
const BUF_SIZE: usize = <Settings as versioned_postcard::Version>::RECORD_MAX + 1;
pub type AppSettings = nvm_settings::SeqNvmSettings<
    Settings,  // uses Settings from versioned-postcard example
    AsyncFlash,
    sequential_storage::cache::NoCache,
    { 126 * 1024 },
    { 2 * 1024 },
    BUF_SIZE,
>;

// creates and owns sequential_storage::MapStorage
let backend = nvm_settings::SeqStorageBackend::new(flash, NoCache::new());

// creates a default AppSettings object
let mut settings = AppSettings::new(backend, nvm_settings::PostcardVersionedFormat);

// multiple references to settings can be shared with a StaticCell wrapper

// load existing settings from flash (if any)
// on error, the settings object is unmodified and here keeps the initial defaults
settings.load().await.inspect_err(|e| error!("Failed to load settings: {}", e)).ok();

let a = settings.get().a;  // read value

// modify in-cache settings, also marks it as dirty
settings.update(|mut s| { s.a = s.a + 1; s });
// actually writes to NVM
settings.commit().await.inspect_err(|e| error!("Settings commit failed: {}", e)).ok();
```
