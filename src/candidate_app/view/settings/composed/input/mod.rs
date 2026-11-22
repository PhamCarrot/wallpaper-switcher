//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
mod action;
mod map;
mod nav_keys;
mod trigger;

pub use action::{ActiveScopes, InputAction};
pub use map::InputMap;
pub use nav_keys::NavKeys;
pub use trigger::{
    binding_config, binding_label, parse_binding, slot_triggers, with_slot, KeyId, KeySpec, Mods,
    MouseButton, MouseSpec, Trigger, WheelDirection, WheelSpec, SLOTS,
};

#[cfg(test)]
mod compatibility_tests;
#[cfg(test)]
mod tests;
