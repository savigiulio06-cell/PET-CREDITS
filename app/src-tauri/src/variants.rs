//! Skins bundled with the UI, listed at compile time from `pets/*/manifest.json`
//! (see `build.rs`). Sorted by id; the first one is the default.

include!(concat!(env!("OUT_DIR"), "/variants.rs"));

pub fn is_known(id: &str) -> bool {
    VARIANTS.iter().any(|(v, _)| *v == id)
}

pub fn default_id() -> &'static str {
    VARIANTS.first().map_or("", |(id, _)| id)
}
