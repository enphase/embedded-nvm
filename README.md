# embedded-nvm

A collection of crates for embedded nonvolatile memory (NVM) operations with a focus on simplifying management of device settings.


## Crates

- `versioned-postcard`: Versioned, migratable postcard records for no_std. Each version is defined with a reference to (and migration function from) the prior version struct. Version stored in a tag as a leading varint. Support for appended new fields migration within the same version tag using Option fields.
- `embedded-nvm-settings`: `NvmSettings` object that presents a in-memory cached settings with modify-update, deferred async commit, agnostic storage backend, and agnostic serialization backend. Built-in adapters to work with versioned-postcard and vanilla postcard.


## Example

The example below defines a versioned settings type with `versioned-postcard`,
then uses `embedded-nvm-settings` to cache, load, update, and commit it.

```rust
//
// VERSIONED-POSTCARD EXAMPLE
//

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


//
// EMBEDDED-NVM-SETTINGS EXAMPLE
//

use embedded_nvm_settings::{SeqNvmSettings, SeqStorageBackend, VersionedPostcardFormat};
const BUF_SIZE: usize = <Settings as Version>::RECORD_MAX + 1;

// boilerplate for mock flash
use sequential_storage::mock_flash::{MockFlashBase, WriteCountCheck};
use sequential_storage::cache::NoCache;
type MockFlash = MockFlashBase<4, 4, 64>;
const FLASH_END: u32 = MockFlash::FULL_FLASH_RANGE.end;

// On real hardware, use your HAL's flash driver, e.g. for STM32:
//   type AsyncFlash = embassy_embedded_hal::adapter::BlockingAsync<
//       embassy_stm32::flash::Flash<'static, embassy_stm32::flash::Blocking>>;
//   let flash = BlockingAsync::new(Flash::new_blocking(p.FLASH));
//   type AppSettings = SeqNvmSettings<Settings, AsyncFlash, NoCache, {126*1024}, {2*1024}, BUF_SIZE>;
let flash = MockFlash::new(WriteCountCheck::Twice, None, true);
type AppSettings = SeqNvmSettings<Settings, MockFlash, NoCache, 0, FLASH_END, BUF_SIZE>;

let backend = SeqStorageBackend::new(flash, NoCache::new());

// this would be in your async firmware code
embassy_futures::block_on(async {
    // creates a default AppSettings object
    // the reference can be shared as operations do not require a mutable reference
    let settings = AppSettings::new(backend, VersionedPostcardFormat);

    // load existing settings from flash (if any)
    // on error, the settings object is unmodified and here keeps the initial defaults
    settings.load().await.ok();

    let a = settings.get().a;  // read value

    // modify in-cache settings, also marks it as dirty
    settings.update(|mut s| { s.a = s.a + 1; s });
    // actually writes to NVM
    settings.commit().await.ok();
});
```


## Example with append-only same-version-tag migration
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
