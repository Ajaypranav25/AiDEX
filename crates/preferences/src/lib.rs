//! Portable, encrypted application preferences.
mod model;
mod service;
pub use model::{AppPreferences, Density, Theme};
pub use service::{PreferencesError, PreferencesService};
