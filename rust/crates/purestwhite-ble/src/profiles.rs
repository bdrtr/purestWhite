//! Remotes known by name whose buttons are not keyboard keys (the SDK's `BleRemoteProfiles.h`):
//! each button's raw code (`RawButton::code`) and the logical key it is. Apps choose the action;
//! the codes stay the raw ones, so bindings saved per button keep working.

/// A logical key of a known remote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteKey {
    Right,
    Left,
    PageDown,
    PageUp,
}

/// One button of a profile.
#[derive(Clone, Copy, Debug)]
pub struct ProfileButton {
    pub code: u32,
    pub key: RemoteKey,
    /// Any report id: the remote's id varies by model, so only the byte and value count.
    pub any_report: bool,
}

/// A remote whose name starts with `name_prefix`.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub name_prefix: &'static str,
    pub buttons: &'static [ProfileButton],
}

const fn button(code: u32, key: RemoteKey, any_report: bool) -> ProfileButton {
    ProfileButton {
        code,
        key,
        any_report,
    }
}

/// The known remotes, in the SDK's order.
pub const PROFILES: [Profile; 2] = [
    // Free3's third button reports byte 1 on tap, byte 0 on hold. Its page buttons keep the
    // generic HID decode. Report ids vary by model.
    Profile {
        name_prefix: "Free3",
        buttons: &[
            button(0x00_0102, RemoteKey::PageDown, true),
            button(0x00_0008, RemoteKey::PageUp, true),
        ],
    },
    // BOOX Keypage has two observed code sets, both on report 3.
    Profile {
        name_prefix: "BOOX Keypage",
        buttons: &[
            button(0x03_0002, RemoteKey::Right, false),
            button(0x03_0001, RemoteKey::Left, false),
            button(0x03_0004, RemoteKey::Right, false),
            button(0x03_0008, RemoteKey::Left, false),
        ],
    },
];

/// The profile of a remote called `name`, its advertised bytes (the first profile whose prefix
/// the name starts with), or none when the generic HID decode applies.
#[must_use]
pub fn profile_index(name: &[u8]) -> Option<usize> {
    PROFILES
        .iter()
        .position(|p| name.starts_with(p.name_prefix.as_bytes()))
}
