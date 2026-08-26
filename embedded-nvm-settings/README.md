# embedded-nvm-settings

Provides a `NvmSettings` object that presents a in-memory cached settings with fast read / modify-update, deferred async commit, agnostic storage backend, and agnostic serialization backend.
Built-in adapters to work with versioned-postcard and vanilla postcard.

`no_std`, no allocator.


## Example

The built-in storage and format backends (which pull in `sequential-storage`, `postcard`, and `versioned-postcard` dependencies) are opt-in with features:
```toml
[dependencies]
embedded-nvm-settings = { version = "0.1", features = ["sequential-storage", "versioned-postcard"] }
# alternatively, use the postcard format instead of versioned-postcard
embedded-nvm-settings = { version = "0.1", features = ["sequential-storage", "postcard"] }
# otherwise, you must implement the `StorageBackend` and `SettingsFormat` traits yourself
```

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

    // append-only schema evolution within the same TAG is also supported, see the versioned-postcard README
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
    // on error, the settings object is unmodified (here, keeps initial defaults)
    settings.load().await.ok();

    let a = settings.get().a;  // read value

    // modify in-cache settings, also marks it as dirty
    settings.update(|mut s| { s.a = s.a + 1; s });
    // actually writes to NVM
    settings.commit().await.ok();
});
```
