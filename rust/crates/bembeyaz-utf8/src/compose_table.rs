// Canonical composition (NFC) pairs, sorted by (base, mark) for binary search.
// Ported from CrossPoint lib/Utf8/Utf8ComposeTable.h (generated from Python unicodedata).

pub(crate) struct ComposeEntry {
    pub base: u16,
    pub mark: u16,
    pub composed: u16,
}

pub(crate) static COMPOSE_TABLE: [ComposeEntry; 847] = [
    ComposeEntry {
        base: 0x0041,
        mark: 0x0300,
        composed: 0x00C0,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0301,
        composed: 0x00C1,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0302,
        composed: 0x00C2,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0303,
        composed: 0x00C3,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0304,
        composed: 0x0100,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0306,
        composed: 0x0102,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0307,
        composed: 0x0226,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0308,
        composed: 0x00C4,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0309,
        composed: 0x1EA2,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x030A,
        composed: 0x00C5,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x030C,
        composed: 0x01CD,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x030F,
        composed: 0x0200,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0311,
        composed: 0x0202,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0323,
        composed: 0x1EA0,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0325,
        composed: 0x1E00,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0328,
        composed: 0x0104,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0340,
        composed: 0x00C0,
    },
    ComposeEntry {
        base: 0x0041,
        mark: 0x0341,
        composed: 0x00C1,
    },
    ComposeEntry {
        base: 0x0042,
        mark: 0x0307,
        composed: 0x1E02,
    },
    ComposeEntry {
        base: 0x0042,
        mark: 0x0323,
        composed: 0x1E04,
    },
    ComposeEntry {
        base: 0x0042,
        mark: 0x0331,
        composed: 0x1E06,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x0301,
        composed: 0x0106,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x0302,
        composed: 0x0108,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x0307,
        composed: 0x010A,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x030C,
        composed: 0x010C,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x0327,
        composed: 0x00C7,
    },
    ComposeEntry {
        base: 0x0043,
        mark: 0x0341,
        composed: 0x0106,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x0307,
        composed: 0x1E0A,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x030C,
        composed: 0x010E,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x0323,
        composed: 0x1E0C,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x0327,
        composed: 0x1E10,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x032D,
        composed: 0x1E12,
    },
    ComposeEntry {
        base: 0x0044,
        mark: 0x0331,
        composed: 0x1E0E,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0300,
        composed: 0x00C8,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0301,
        composed: 0x00C9,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0302,
        composed: 0x00CA,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0303,
        composed: 0x1EBC,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0304,
        composed: 0x0112,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0306,
        composed: 0x0114,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0307,
        composed: 0x0116,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0308,
        composed: 0x00CB,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0309,
        composed: 0x1EBA,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x030C,
        composed: 0x011A,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x030F,
        composed: 0x0204,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0311,
        composed: 0x0206,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0323,
        composed: 0x1EB8,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0327,
        composed: 0x0228,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0328,
        composed: 0x0118,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x032D,
        composed: 0x1E18,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0330,
        composed: 0x1E1A,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0340,
        composed: 0x00C8,
    },
    ComposeEntry {
        base: 0x0045,
        mark: 0x0341,
        composed: 0x00C9,
    },
    ComposeEntry {
        base: 0x0046,
        mark: 0x0307,
        composed: 0x1E1E,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0301,
        composed: 0x01F4,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0302,
        composed: 0x011C,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0304,
        composed: 0x1E20,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0306,
        composed: 0x011E,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0307,
        composed: 0x0120,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x030C,
        composed: 0x01E6,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0327,
        composed: 0x0122,
    },
    ComposeEntry {
        base: 0x0047,
        mark: 0x0341,
        composed: 0x01F4,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x0302,
        composed: 0x0124,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x0307,
        composed: 0x1E22,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x0308,
        composed: 0x1E26,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x030C,
        composed: 0x021E,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x0323,
        composed: 0x1E24,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x0327,
        composed: 0x1E28,
    },
    ComposeEntry {
        base: 0x0048,
        mark: 0x032E,
        composed: 0x1E2A,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0300,
        composed: 0x00CC,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0301,
        composed: 0x00CD,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0302,
        composed: 0x00CE,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0303,
        composed: 0x0128,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0304,
        composed: 0x012A,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0306,
        composed: 0x012C,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0307,
        composed: 0x0130,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0308,
        composed: 0x00CF,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0309,
        composed: 0x1EC8,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x030C,
        composed: 0x01CF,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x030F,
        composed: 0x0208,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0311,
        composed: 0x020A,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0323,
        composed: 0x1ECA,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0328,
        composed: 0x012E,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0330,
        composed: 0x1E2C,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0340,
        composed: 0x00CC,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0341,
        composed: 0x00CD,
    },
    ComposeEntry {
        base: 0x0049,
        mark: 0x0344,
        composed: 0x1E2E,
    },
    ComposeEntry {
        base: 0x004A,
        mark: 0x0302,
        composed: 0x0134,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x0301,
        composed: 0x1E30,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x030C,
        composed: 0x01E8,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x0323,
        composed: 0x1E32,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x0327,
        composed: 0x0136,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x0331,
        composed: 0x1E34,
    },
    ComposeEntry {
        base: 0x004B,
        mark: 0x0341,
        composed: 0x1E30,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x0301,
        composed: 0x0139,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x030C,
        composed: 0x013D,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x0323,
        composed: 0x1E36,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x0327,
        composed: 0x013B,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x032D,
        composed: 0x1E3C,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x0331,
        composed: 0x1E3A,
    },
    ComposeEntry {
        base: 0x004C,
        mark: 0x0341,
        composed: 0x0139,
    },
    ComposeEntry {
        base: 0x004D,
        mark: 0x0301,
        composed: 0x1E3E,
    },
    ComposeEntry {
        base: 0x004D,
        mark: 0x0307,
        composed: 0x1E40,
    },
    ComposeEntry {
        base: 0x004D,
        mark: 0x0323,
        composed: 0x1E42,
    },
    ComposeEntry {
        base: 0x004D,
        mark: 0x0341,
        composed: 0x1E3E,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0300,
        composed: 0x01F8,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0301,
        composed: 0x0143,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0303,
        composed: 0x00D1,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0307,
        composed: 0x1E44,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x030C,
        composed: 0x0147,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0323,
        composed: 0x1E46,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0327,
        composed: 0x0145,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x032D,
        composed: 0x1E4A,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0331,
        composed: 0x1E48,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0340,
        composed: 0x01F8,
    },
    ComposeEntry {
        base: 0x004E,
        mark: 0x0341,
        composed: 0x0143,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0300,
        composed: 0x00D2,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0301,
        composed: 0x00D3,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0302,
        composed: 0x00D4,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0303,
        composed: 0x00D5,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0304,
        composed: 0x014C,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0306,
        composed: 0x014E,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0307,
        composed: 0x022E,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0308,
        composed: 0x00D6,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0309,
        composed: 0x1ECE,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x030B,
        composed: 0x0150,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x030C,
        composed: 0x01D1,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x030F,
        composed: 0x020C,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0311,
        composed: 0x020E,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x031B,
        composed: 0x01A0,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0323,
        composed: 0x1ECC,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0328,
        composed: 0x01EA,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0340,
        composed: 0x00D2,
    },
    ComposeEntry {
        base: 0x004F,
        mark: 0x0341,
        composed: 0x00D3,
    },
    ComposeEntry {
        base: 0x0050,
        mark: 0x0301,
        composed: 0x1E54,
    },
    ComposeEntry {
        base: 0x0050,
        mark: 0x0307,
        composed: 0x1E56,
    },
    ComposeEntry {
        base: 0x0050,
        mark: 0x0341,
        composed: 0x1E54,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0301,
        composed: 0x0154,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0307,
        composed: 0x1E58,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x030C,
        composed: 0x0158,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x030F,
        composed: 0x0210,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0311,
        composed: 0x0212,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0323,
        composed: 0x1E5A,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0327,
        composed: 0x0156,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0331,
        composed: 0x1E5E,
    },
    ComposeEntry {
        base: 0x0052,
        mark: 0x0341,
        composed: 0x0154,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0301,
        composed: 0x015A,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0302,
        composed: 0x015C,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0307,
        composed: 0x1E60,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x030C,
        composed: 0x0160,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0323,
        composed: 0x1E62,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0326,
        composed: 0x0218,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0327,
        composed: 0x015E,
    },
    ComposeEntry {
        base: 0x0053,
        mark: 0x0341,
        composed: 0x015A,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x0307,
        composed: 0x1E6A,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x030C,
        composed: 0x0164,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x0323,
        composed: 0x1E6C,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x0326,
        composed: 0x021A,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x0327,
        composed: 0x0162,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x032D,
        composed: 0x1E70,
    },
    ComposeEntry {
        base: 0x0054,
        mark: 0x0331,
        composed: 0x1E6E,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0300,
        composed: 0x00D9,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0301,
        composed: 0x00DA,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0302,
        composed: 0x00DB,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0303,
        composed: 0x0168,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0304,
        composed: 0x016A,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0306,
        composed: 0x016C,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0308,
        composed: 0x00DC,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0309,
        composed: 0x1EE6,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x030A,
        composed: 0x016E,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x030B,
        composed: 0x0170,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x030C,
        composed: 0x01D3,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x030F,
        composed: 0x0214,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0311,
        composed: 0x0216,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x031B,
        composed: 0x01AF,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0323,
        composed: 0x1EE4,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0324,
        composed: 0x1E72,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0328,
        composed: 0x0172,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x032D,
        composed: 0x1E76,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0330,
        composed: 0x1E74,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0340,
        composed: 0x00D9,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0341,
        composed: 0x00DA,
    },
    ComposeEntry {
        base: 0x0055,
        mark: 0x0344,
        composed: 0x01D7,
    },
    ComposeEntry {
        base: 0x0056,
        mark: 0x0303,
        composed: 0x1E7C,
    },
    ComposeEntry {
        base: 0x0056,
        mark: 0x0323,
        composed: 0x1E7E,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0300,
        composed: 0x1E80,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0301,
        composed: 0x1E82,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0302,
        composed: 0x0174,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0307,
        composed: 0x1E86,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0308,
        composed: 0x1E84,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0323,
        composed: 0x1E88,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0340,
        composed: 0x1E80,
    },
    ComposeEntry {
        base: 0x0057,
        mark: 0x0341,
        composed: 0x1E82,
    },
    ComposeEntry {
        base: 0x0058,
        mark: 0x0307,
        composed: 0x1E8A,
    },
    ComposeEntry {
        base: 0x0058,
        mark: 0x0308,
        composed: 0x1E8C,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0300,
        composed: 0x1EF2,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0301,
        composed: 0x00DD,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0302,
        composed: 0x0176,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0303,
        composed: 0x1EF8,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0304,
        composed: 0x0232,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0307,
        composed: 0x1E8E,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0308,
        composed: 0x0178,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0309,
        composed: 0x1EF6,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0323,
        composed: 0x1EF4,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0340,
        composed: 0x1EF2,
    },
    ComposeEntry {
        base: 0x0059,
        mark: 0x0341,
        composed: 0x00DD,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0301,
        composed: 0x0179,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0302,
        composed: 0x1E90,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0307,
        composed: 0x017B,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x030C,
        composed: 0x017D,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0323,
        composed: 0x1E92,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0331,
        composed: 0x1E94,
    },
    ComposeEntry {
        base: 0x005A,
        mark: 0x0341,
        composed: 0x0179,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0300,
        composed: 0x00E0,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0301,
        composed: 0x00E1,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0302,
        composed: 0x00E2,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0303,
        composed: 0x00E3,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0304,
        composed: 0x0101,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0306,
        composed: 0x0103,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0307,
        composed: 0x0227,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0308,
        composed: 0x00E4,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0309,
        composed: 0x1EA3,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x030A,
        composed: 0x00E5,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x030C,
        composed: 0x01CE,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x030F,
        composed: 0x0201,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0311,
        composed: 0x0203,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0323,
        composed: 0x1EA1,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0325,
        composed: 0x1E01,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0328,
        composed: 0x0105,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0340,
        composed: 0x00E0,
    },
    ComposeEntry {
        base: 0x0061,
        mark: 0x0341,
        composed: 0x00E1,
    },
    ComposeEntry {
        base: 0x0062,
        mark: 0x0307,
        composed: 0x1E03,
    },
    ComposeEntry {
        base: 0x0062,
        mark: 0x0323,
        composed: 0x1E05,
    },
    ComposeEntry {
        base: 0x0062,
        mark: 0x0331,
        composed: 0x1E07,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x0301,
        composed: 0x0107,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x0302,
        composed: 0x0109,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x0307,
        composed: 0x010B,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x030C,
        composed: 0x010D,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x0327,
        composed: 0x00E7,
    },
    ComposeEntry {
        base: 0x0063,
        mark: 0x0341,
        composed: 0x0107,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x0307,
        composed: 0x1E0B,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x030C,
        composed: 0x010F,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x0323,
        composed: 0x1E0D,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x0327,
        composed: 0x1E11,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x032D,
        composed: 0x1E13,
    },
    ComposeEntry {
        base: 0x0064,
        mark: 0x0331,
        composed: 0x1E0F,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0300,
        composed: 0x00E8,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0301,
        composed: 0x00E9,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0302,
        composed: 0x00EA,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0303,
        composed: 0x1EBD,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0304,
        composed: 0x0113,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0306,
        composed: 0x0115,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0307,
        composed: 0x0117,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0308,
        composed: 0x00EB,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0309,
        composed: 0x1EBB,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x030C,
        composed: 0x011B,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x030F,
        composed: 0x0205,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0311,
        composed: 0x0207,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0323,
        composed: 0x1EB9,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0327,
        composed: 0x0229,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0328,
        composed: 0x0119,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x032D,
        composed: 0x1E19,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0330,
        composed: 0x1E1B,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0340,
        composed: 0x00E8,
    },
    ComposeEntry {
        base: 0x0065,
        mark: 0x0341,
        composed: 0x00E9,
    },
    ComposeEntry {
        base: 0x0066,
        mark: 0x0307,
        composed: 0x1E1F,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0301,
        composed: 0x01F5,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0302,
        composed: 0x011D,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0304,
        composed: 0x1E21,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0306,
        composed: 0x011F,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0307,
        composed: 0x0121,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x030C,
        composed: 0x01E7,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0327,
        composed: 0x0123,
    },
    ComposeEntry {
        base: 0x0067,
        mark: 0x0341,
        composed: 0x01F5,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0302,
        composed: 0x0125,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0307,
        composed: 0x1E23,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0308,
        composed: 0x1E27,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x030C,
        composed: 0x021F,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0323,
        composed: 0x1E25,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0327,
        composed: 0x1E29,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x032E,
        composed: 0x1E2B,
    },
    ComposeEntry {
        base: 0x0068,
        mark: 0x0331,
        composed: 0x1E96,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0300,
        composed: 0x00EC,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0301,
        composed: 0x00ED,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0302,
        composed: 0x00EE,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0303,
        composed: 0x0129,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0304,
        composed: 0x012B,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0306,
        composed: 0x012D,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0308,
        composed: 0x00EF,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0309,
        composed: 0x1EC9,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x030C,
        composed: 0x01D0,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x030F,
        composed: 0x0209,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0311,
        composed: 0x020B,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0323,
        composed: 0x1ECB,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0328,
        composed: 0x012F,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0330,
        composed: 0x1E2D,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0340,
        composed: 0x00EC,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0341,
        composed: 0x00ED,
    },
    ComposeEntry {
        base: 0x0069,
        mark: 0x0344,
        composed: 0x1E2F,
    },
    ComposeEntry {
        base: 0x006A,
        mark: 0x0302,
        composed: 0x0135,
    },
    ComposeEntry {
        base: 0x006A,
        mark: 0x030C,
        composed: 0x01F0,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x0301,
        composed: 0x1E31,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x030C,
        composed: 0x01E9,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x0323,
        composed: 0x1E33,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x0327,
        composed: 0x0137,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x0331,
        composed: 0x1E35,
    },
    ComposeEntry {
        base: 0x006B,
        mark: 0x0341,
        composed: 0x1E31,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x0301,
        composed: 0x013A,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x030C,
        composed: 0x013E,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x0323,
        composed: 0x1E37,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x0327,
        composed: 0x013C,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x032D,
        composed: 0x1E3D,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x0331,
        composed: 0x1E3B,
    },
    ComposeEntry {
        base: 0x006C,
        mark: 0x0341,
        composed: 0x013A,
    },
    ComposeEntry {
        base: 0x006D,
        mark: 0x0301,
        composed: 0x1E3F,
    },
    ComposeEntry {
        base: 0x006D,
        mark: 0x0307,
        composed: 0x1E41,
    },
    ComposeEntry {
        base: 0x006D,
        mark: 0x0323,
        composed: 0x1E43,
    },
    ComposeEntry {
        base: 0x006D,
        mark: 0x0341,
        composed: 0x1E3F,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0300,
        composed: 0x01F9,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0301,
        composed: 0x0144,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0303,
        composed: 0x00F1,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0307,
        composed: 0x1E45,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x030C,
        composed: 0x0148,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0323,
        composed: 0x1E47,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0327,
        composed: 0x0146,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x032D,
        composed: 0x1E4B,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0331,
        composed: 0x1E49,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0340,
        composed: 0x01F9,
    },
    ComposeEntry {
        base: 0x006E,
        mark: 0x0341,
        composed: 0x0144,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0300,
        composed: 0x00F2,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0301,
        composed: 0x00F3,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0302,
        composed: 0x00F4,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0303,
        composed: 0x00F5,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0304,
        composed: 0x014D,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0306,
        composed: 0x014F,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0307,
        composed: 0x022F,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0308,
        composed: 0x00F6,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0309,
        composed: 0x1ECF,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x030B,
        composed: 0x0151,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x030C,
        composed: 0x01D2,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x030F,
        composed: 0x020D,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0311,
        composed: 0x020F,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x031B,
        composed: 0x01A1,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0323,
        composed: 0x1ECD,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0328,
        composed: 0x01EB,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0340,
        composed: 0x00F2,
    },
    ComposeEntry {
        base: 0x006F,
        mark: 0x0341,
        composed: 0x00F3,
    },
    ComposeEntry {
        base: 0x0070,
        mark: 0x0301,
        composed: 0x1E55,
    },
    ComposeEntry {
        base: 0x0070,
        mark: 0x0307,
        composed: 0x1E57,
    },
    ComposeEntry {
        base: 0x0070,
        mark: 0x0341,
        composed: 0x1E55,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0301,
        composed: 0x0155,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0307,
        composed: 0x1E59,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x030C,
        composed: 0x0159,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x030F,
        composed: 0x0211,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0311,
        composed: 0x0213,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0323,
        composed: 0x1E5B,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0327,
        composed: 0x0157,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0331,
        composed: 0x1E5F,
    },
    ComposeEntry {
        base: 0x0072,
        mark: 0x0341,
        composed: 0x0155,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0301,
        composed: 0x015B,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0302,
        composed: 0x015D,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0307,
        composed: 0x1E61,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x030C,
        composed: 0x0161,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0323,
        composed: 0x1E63,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0326,
        composed: 0x0219,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0327,
        composed: 0x015F,
    },
    ComposeEntry {
        base: 0x0073,
        mark: 0x0341,
        composed: 0x015B,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0307,
        composed: 0x1E6B,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0308,
        composed: 0x1E97,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x030C,
        composed: 0x0165,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0323,
        composed: 0x1E6D,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0326,
        composed: 0x021B,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0327,
        composed: 0x0163,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x032D,
        composed: 0x1E71,
    },
    ComposeEntry {
        base: 0x0074,
        mark: 0x0331,
        composed: 0x1E6F,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0300,
        composed: 0x00F9,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0301,
        composed: 0x00FA,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0302,
        composed: 0x00FB,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0303,
        composed: 0x0169,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0304,
        composed: 0x016B,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0306,
        composed: 0x016D,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0308,
        composed: 0x00FC,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0309,
        composed: 0x1EE7,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x030A,
        composed: 0x016F,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x030B,
        composed: 0x0171,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x030C,
        composed: 0x01D4,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x030F,
        composed: 0x0215,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0311,
        composed: 0x0217,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x031B,
        composed: 0x01B0,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0323,
        composed: 0x1EE5,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0324,
        composed: 0x1E73,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0328,
        composed: 0x0173,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x032D,
        composed: 0x1E77,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0330,
        composed: 0x1E75,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0340,
        composed: 0x00F9,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0341,
        composed: 0x00FA,
    },
    ComposeEntry {
        base: 0x0075,
        mark: 0x0344,
        composed: 0x01D8,
    },
    ComposeEntry {
        base: 0x0076,
        mark: 0x0303,
        composed: 0x1E7D,
    },
    ComposeEntry {
        base: 0x0076,
        mark: 0x0323,
        composed: 0x1E7F,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0300,
        composed: 0x1E81,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0301,
        composed: 0x1E83,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0302,
        composed: 0x0175,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0307,
        composed: 0x1E87,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0308,
        composed: 0x1E85,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x030A,
        composed: 0x1E98,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0323,
        composed: 0x1E89,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0340,
        composed: 0x1E81,
    },
    ComposeEntry {
        base: 0x0077,
        mark: 0x0341,
        composed: 0x1E83,
    },
    ComposeEntry {
        base: 0x0078,
        mark: 0x0307,
        composed: 0x1E8B,
    },
    ComposeEntry {
        base: 0x0078,
        mark: 0x0308,
        composed: 0x1E8D,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0300,
        composed: 0x1EF3,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0301,
        composed: 0x00FD,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0302,
        composed: 0x0177,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0303,
        composed: 0x1EF9,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0304,
        composed: 0x0233,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0307,
        composed: 0x1E8F,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0308,
        composed: 0x00FF,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0309,
        composed: 0x1EF7,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x030A,
        composed: 0x1E99,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0323,
        composed: 0x1EF5,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0340,
        composed: 0x1EF3,
    },
    ComposeEntry {
        base: 0x0079,
        mark: 0x0341,
        composed: 0x00FD,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0301,
        composed: 0x017A,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0302,
        composed: 0x1E91,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0307,
        composed: 0x017C,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x030C,
        composed: 0x017E,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0323,
        composed: 0x1E93,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0331,
        composed: 0x1E95,
    },
    ComposeEntry {
        base: 0x007A,
        mark: 0x0341,
        composed: 0x017A,
    },
    ComposeEntry {
        base: 0x00A8,
        mark: 0x0300,
        composed: 0x1FED,
    },
    ComposeEntry {
        base: 0x00A8,
        mark: 0x0301,
        composed: 0x0385,
    },
    ComposeEntry {
        base: 0x00A8,
        mark: 0x0340,
        composed: 0x1FED,
    },
    ComposeEntry {
        base: 0x00A8,
        mark: 0x0341,
        composed: 0x0385,
    },
    ComposeEntry {
        base: 0x00A8,
        mark: 0x0342,
        composed: 0x1FC1,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0300,
        composed: 0x1EA6,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0301,
        composed: 0x1EA4,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0303,
        composed: 0x1EAA,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0309,
        composed: 0x1EA8,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0323,
        composed: 0x1EAC,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0340,
        composed: 0x1EA6,
    },
    ComposeEntry {
        base: 0x00C2,
        mark: 0x0341,
        composed: 0x1EA4,
    },
    ComposeEntry {
        base: 0x00C4,
        mark: 0x0304,
        composed: 0x01DE,
    },
    ComposeEntry {
        base: 0x00C5,
        mark: 0x0301,
        composed: 0x01FA,
    },
    ComposeEntry {
        base: 0x00C5,
        mark: 0x0341,
        composed: 0x01FA,
    },
    ComposeEntry {
        base: 0x00C6,
        mark: 0x0301,
        composed: 0x01FC,
    },
    ComposeEntry {
        base: 0x00C6,
        mark: 0x0304,
        composed: 0x01E2,
    },
    ComposeEntry {
        base: 0x00C6,
        mark: 0x0341,
        composed: 0x01FC,
    },
    ComposeEntry {
        base: 0x00C7,
        mark: 0x0301,
        composed: 0x1E08,
    },
    ComposeEntry {
        base: 0x00C7,
        mark: 0x0341,
        composed: 0x1E08,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0300,
        composed: 0x1EC0,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0301,
        composed: 0x1EBE,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0303,
        composed: 0x1EC4,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0309,
        composed: 0x1EC2,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0323,
        composed: 0x1EC6,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0340,
        composed: 0x1EC0,
    },
    ComposeEntry {
        base: 0x00CA,
        mark: 0x0341,
        composed: 0x1EBE,
    },
    ComposeEntry {
        base: 0x00CF,
        mark: 0x0301,
        composed: 0x1E2E,
    },
    ComposeEntry {
        base: 0x00CF,
        mark: 0x0341,
        composed: 0x1E2E,
    },
    ComposeEntry {
        base: 0x00D2,
        mark: 0x031B,
        composed: 0x1EDC,
    },
    ComposeEntry {
        base: 0x00D3,
        mark: 0x031B,
        composed: 0x1EDA,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0300,
        composed: 0x1ED2,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0301,
        composed: 0x1ED0,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0303,
        composed: 0x1ED6,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0309,
        composed: 0x1ED4,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0323,
        composed: 0x1ED8,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0340,
        composed: 0x1ED2,
    },
    ComposeEntry {
        base: 0x00D4,
        mark: 0x0341,
        composed: 0x1ED0,
    },
    ComposeEntry {
        base: 0x00D5,
        mark: 0x0301,
        composed: 0x1E4C,
    },
    ComposeEntry {
        base: 0x00D5,
        mark: 0x0304,
        composed: 0x022C,
    },
    ComposeEntry {
        base: 0x00D5,
        mark: 0x0308,
        composed: 0x1E4E,
    },
    ComposeEntry {
        base: 0x00D5,
        mark: 0x031B,
        composed: 0x1EE0,
    },
    ComposeEntry {
        base: 0x00D5,
        mark: 0x0341,
        composed: 0x1E4C,
    },
    ComposeEntry {
        base: 0x00D6,
        mark: 0x0304,
        composed: 0x022A,
    },
    ComposeEntry {
        base: 0x00D8,
        mark: 0x0301,
        composed: 0x01FE,
    },
    ComposeEntry {
        base: 0x00D8,
        mark: 0x0341,
        composed: 0x01FE,
    },
    ComposeEntry {
        base: 0x00D9,
        mark: 0x031B,
        composed: 0x1EEA,
    },
    ComposeEntry {
        base: 0x00DA,
        mark: 0x031B,
        composed: 0x1EE8,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x0300,
        composed: 0x01DB,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x0301,
        composed: 0x01D7,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x0304,
        composed: 0x01D5,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x030C,
        composed: 0x01D9,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x0340,
        composed: 0x01DB,
    },
    ComposeEntry {
        base: 0x00DC,
        mark: 0x0341,
        composed: 0x01D7,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0300,
        composed: 0x1EA7,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0301,
        composed: 0x1EA5,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0303,
        composed: 0x1EAB,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0309,
        composed: 0x1EA9,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0323,
        composed: 0x1EAD,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0340,
        composed: 0x1EA7,
    },
    ComposeEntry {
        base: 0x00E2,
        mark: 0x0341,
        composed: 0x1EA5,
    },
    ComposeEntry {
        base: 0x00E4,
        mark: 0x0304,
        composed: 0x01DF,
    },
    ComposeEntry {
        base: 0x00E5,
        mark: 0x0301,
        composed: 0x01FB,
    },
    ComposeEntry {
        base: 0x00E5,
        mark: 0x0341,
        composed: 0x01FB,
    },
    ComposeEntry {
        base: 0x00E6,
        mark: 0x0301,
        composed: 0x01FD,
    },
    ComposeEntry {
        base: 0x00E6,
        mark: 0x0304,
        composed: 0x01E3,
    },
    ComposeEntry {
        base: 0x00E6,
        mark: 0x0341,
        composed: 0x01FD,
    },
    ComposeEntry {
        base: 0x00E7,
        mark: 0x0301,
        composed: 0x1E09,
    },
    ComposeEntry {
        base: 0x00E7,
        mark: 0x0341,
        composed: 0x1E09,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0300,
        composed: 0x1EC1,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0301,
        composed: 0x1EBF,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0303,
        composed: 0x1EC5,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0309,
        composed: 0x1EC3,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0323,
        composed: 0x1EC7,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0340,
        composed: 0x1EC1,
    },
    ComposeEntry {
        base: 0x00EA,
        mark: 0x0341,
        composed: 0x1EBF,
    },
    ComposeEntry {
        base: 0x00EF,
        mark: 0x0301,
        composed: 0x1E2F,
    },
    ComposeEntry {
        base: 0x00EF,
        mark: 0x0341,
        composed: 0x1E2F,
    },
    ComposeEntry {
        base: 0x00F2,
        mark: 0x031B,
        composed: 0x1EDD,
    },
    ComposeEntry {
        base: 0x00F3,
        mark: 0x031B,
        composed: 0x1EDB,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0300,
        composed: 0x1ED3,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0301,
        composed: 0x1ED1,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0303,
        composed: 0x1ED7,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0309,
        composed: 0x1ED5,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0323,
        composed: 0x1ED9,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0340,
        composed: 0x1ED3,
    },
    ComposeEntry {
        base: 0x00F4,
        mark: 0x0341,
        composed: 0x1ED1,
    },
    ComposeEntry {
        base: 0x00F5,
        mark: 0x0301,
        composed: 0x1E4D,
    },
    ComposeEntry {
        base: 0x00F5,
        mark: 0x0304,
        composed: 0x022D,
    },
    ComposeEntry {
        base: 0x00F5,
        mark: 0x0308,
        composed: 0x1E4F,
    },
    ComposeEntry {
        base: 0x00F5,
        mark: 0x031B,
        composed: 0x1EE1,
    },
    ComposeEntry {
        base: 0x00F5,
        mark: 0x0341,
        composed: 0x1E4D,
    },
    ComposeEntry {
        base: 0x00F6,
        mark: 0x0304,
        composed: 0x022B,
    },
    ComposeEntry {
        base: 0x00F8,
        mark: 0x0301,
        composed: 0x01FF,
    },
    ComposeEntry {
        base: 0x00F8,
        mark: 0x0341,
        composed: 0x01FF,
    },
    ComposeEntry {
        base: 0x00F9,
        mark: 0x031B,
        composed: 0x1EEB,
    },
    ComposeEntry {
        base: 0x00FA,
        mark: 0x031B,
        composed: 0x1EE9,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x0300,
        composed: 0x01DC,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x0301,
        composed: 0x01D8,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x0304,
        composed: 0x01D6,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x030C,
        composed: 0x01DA,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x0340,
        composed: 0x01DC,
    },
    ComposeEntry {
        base: 0x00FC,
        mark: 0x0341,
        composed: 0x01D8,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0300,
        composed: 0x1EB0,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0301,
        composed: 0x1EAE,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0303,
        composed: 0x1EB4,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0309,
        composed: 0x1EB2,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0323,
        composed: 0x1EB6,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0340,
        composed: 0x1EB0,
    },
    ComposeEntry {
        base: 0x0102,
        mark: 0x0341,
        composed: 0x1EAE,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0300,
        composed: 0x1EB1,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0301,
        composed: 0x1EAF,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0303,
        composed: 0x1EB5,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0309,
        composed: 0x1EB3,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0323,
        composed: 0x1EB7,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0340,
        composed: 0x1EB1,
    },
    ComposeEntry {
        base: 0x0103,
        mark: 0x0341,
        composed: 0x1EAF,
    },
    ComposeEntry {
        base: 0x0106,
        mark: 0x0327,
        composed: 0x1E08,
    },
    ComposeEntry {
        base: 0x0107,
        mark: 0x0327,
        composed: 0x1E09,
    },
    ComposeEntry {
        base: 0x0112,
        mark: 0x0300,
        composed: 0x1E14,
    },
    ComposeEntry {
        base: 0x0112,
        mark: 0x0301,
        composed: 0x1E16,
    },
    ComposeEntry {
        base: 0x0112,
        mark: 0x0340,
        composed: 0x1E14,
    },
    ComposeEntry {
        base: 0x0112,
        mark: 0x0341,
        composed: 0x1E16,
    },
    ComposeEntry {
        base: 0x0113,
        mark: 0x0300,
        composed: 0x1E15,
    },
    ComposeEntry {
        base: 0x0113,
        mark: 0x0301,
        composed: 0x1E17,
    },
    ComposeEntry {
        base: 0x0113,
        mark: 0x0340,
        composed: 0x1E15,
    },
    ComposeEntry {
        base: 0x0113,
        mark: 0x0341,
        composed: 0x1E17,
    },
    ComposeEntry {
        base: 0x0114,
        mark: 0x0327,
        composed: 0x1E1C,
    },
    ComposeEntry {
        base: 0x0115,
        mark: 0x0327,
        composed: 0x1E1D,
    },
    ComposeEntry {
        base: 0x014C,
        mark: 0x0300,
        composed: 0x1E50,
    },
    ComposeEntry {
        base: 0x014C,
        mark: 0x0301,
        composed: 0x1E52,
    },
    ComposeEntry {
        base: 0x014C,
        mark: 0x0328,
        composed: 0x01EC,
    },
    ComposeEntry {
        base: 0x014C,
        mark: 0x0340,
        composed: 0x1E50,
    },
    ComposeEntry {
        base: 0x014C,
        mark: 0x0341,
        composed: 0x1E52,
    },
    ComposeEntry {
        base: 0x014D,
        mark: 0x0300,
        composed: 0x1E51,
    },
    ComposeEntry {
        base: 0x014D,
        mark: 0x0301,
        composed: 0x1E53,
    },
    ComposeEntry {
        base: 0x014D,
        mark: 0x0328,
        composed: 0x01ED,
    },
    ComposeEntry {
        base: 0x014D,
        mark: 0x0340,
        composed: 0x1E51,
    },
    ComposeEntry {
        base: 0x014D,
        mark: 0x0341,
        composed: 0x1E53,
    },
    ComposeEntry {
        base: 0x015A,
        mark: 0x0307,
        composed: 0x1E64,
    },
    ComposeEntry {
        base: 0x015B,
        mark: 0x0307,
        composed: 0x1E65,
    },
    ComposeEntry {
        base: 0x0160,
        mark: 0x0307,
        composed: 0x1E66,
    },
    ComposeEntry {
        base: 0x0161,
        mark: 0x0307,
        composed: 0x1E67,
    },
    ComposeEntry {
        base: 0x0168,
        mark: 0x0301,
        composed: 0x1E78,
    },
    ComposeEntry {
        base: 0x0168,
        mark: 0x031B,
        composed: 0x1EEE,
    },
    ComposeEntry {
        base: 0x0168,
        mark: 0x0341,
        composed: 0x1E78,
    },
    ComposeEntry {
        base: 0x0169,
        mark: 0x0301,
        composed: 0x1E79,
    },
    ComposeEntry {
        base: 0x0169,
        mark: 0x031B,
        composed: 0x1EEF,
    },
    ComposeEntry {
        base: 0x0169,
        mark: 0x0341,
        composed: 0x1E79,
    },
    ComposeEntry {
        base: 0x016A,
        mark: 0x0308,
        composed: 0x1E7A,
    },
    ComposeEntry {
        base: 0x016B,
        mark: 0x0308,
        composed: 0x1E7B,
    },
    ComposeEntry {
        base: 0x017F,
        mark: 0x0307,
        composed: 0x1E9B,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0300,
        composed: 0x1EDC,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0301,
        composed: 0x1EDA,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0303,
        composed: 0x1EE0,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0309,
        composed: 0x1EDE,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0323,
        composed: 0x1EE2,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0340,
        composed: 0x1EDC,
    },
    ComposeEntry {
        base: 0x01A0,
        mark: 0x0341,
        composed: 0x1EDA,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0300,
        composed: 0x1EDD,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0301,
        composed: 0x1EDB,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0303,
        composed: 0x1EE1,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0309,
        composed: 0x1EDF,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0323,
        composed: 0x1EE3,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0340,
        composed: 0x1EDD,
    },
    ComposeEntry {
        base: 0x01A1,
        mark: 0x0341,
        composed: 0x1EDB,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0300,
        composed: 0x1EEA,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0301,
        composed: 0x1EE8,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0303,
        composed: 0x1EEE,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0309,
        composed: 0x1EEC,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0323,
        composed: 0x1EF0,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0340,
        composed: 0x1EEA,
    },
    ComposeEntry {
        base: 0x01AF,
        mark: 0x0341,
        composed: 0x1EE8,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0300,
        composed: 0x1EEB,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0301,
        composed: 0x1EE9,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0303,
        composed: 0x1EEF,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0309,
        composed: 0x1EED,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0323,
        composed: 0x1EF1,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0340,
        composed: 0x1EEB,
    },
    ComposeEntry {
        base: 0x01B0,
        mark: 0x0341,
        composed: 0x1EE9,
    },
    ComposeEntry {
        base: 0x01B7,
        mark: 0x030C,
        composed: 0x01EE,
    },
    ComposeEntry {
        base: 0x01EA,
        mark: 0x0304,
        composed: 0x01EC,
    },
    ComposeEntry {
        base: 0x01EB,
        mark: 0x0304,
        composed: 0x01ED,
    },
    ComposeEntry {
        base: 0x0226,
        mark: 0x0304,
        composed: 0x01E0,
    },
    ComposeEntry {
        base: 0x0227,
        mark: 0x0304,
        composed: 0x01E1,
    },
    ComposeEntry {
        base: 0x0228,
        mark: 0x0306,
        composed: 0x1E1C,
    },
    ComposeEntry {
        base: 0x0229,
        mark: 0x0306,
        composed: 0x1E1D,
    },
    ComposeEntry {
        base: 0x022E,
        mark: 0x0304,
        composed: 0x0230,
    },
    ComposeEntry {
        base: 0x022F,
        mark: 0x0304,
        composed: 0x0231,
    },
    ComposeEntry {
        base: 0x0292,
        mark: 0x030C,
        composed: 0x01EF,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0300,
        composed: 0x1FBA,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0301,
        composed: 0x0386,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0304,
        composed: 0x1FB9,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0306,
        composed: 0x1FB8,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0313,
        composed: 0x1F08,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0314,
        composed: 0x1F09,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0340,
        composed: 0x1FBA,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0341,
        composed: 0x0386,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0343,
        composed: 0x1F08,
    },
    ComposeEntry {
        base: 0x0391,
        mark: 0x0345,
        composed: 0x1FBC,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0300,
        composed: 0x1FC8,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0301,
        composed: 0x0388,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0313,
        composed: 0x1F18,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0314,
        composed: 0x1F19,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0340,
        composed: 0x1FC8,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0341,
        composed: 0x0388,
    },
    ComposeEntry {
        base: 0x0395,
        mark: 0x0343,
        composed: 0x1F18,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0300,
        composed: 0x1FCA,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0301,
        composed: 0x0389,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0313,
        composed: 0x1F28,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0314,
        composed: 0x1F29,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0340,
        composed: 0x1FCA,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0341,
        composed: 0x0389,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0343,
        composed: 0x1F28,
    },
    ComposeEntry {
        base: 0x0397,
        mark: 0x0345,
        composed: 0x1FCC,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0300,
        composed: 0x1FDA,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0301,
        composed: 0x038A,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0304,
        composed: 0x1FD9,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0306,
        composed: 0x1FD8,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0308,
        composed: 0x03AA,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0313,
        composed: 0x1F38,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0314,
        composed: 0x1F39,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0340,
        composed: 0x1FDA,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0341,
        composed: 0x038A,
    },
    ComposeEntry {
        base: 0x0399,
        mark: 0x0343,
        composed: 0x1F38,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0300,
        composed: 0x1FF8,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0301,
        composed: 0x038C,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0313,
        composed: 0x1F48,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0314,
        composed: 0x1F49,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0340,
        composed: 0x1FF8,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0341,
        composed: 0x038C,
    },
    ComposeEntry {
        base: 0x039F,
        mark: 0x0343,
        composed: 0x1F48,
    },
    ComposeEntry {
        base: 0x03A1,
        mark: 0x0314,
        composed: 0x1FEC,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0300,
        composed: 0x1FEA,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0301,
        composed: 0x038E,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0304,
        composed: 0x1FE9,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0306,
        composed: 0x1FE8,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0308,
        composed: 0x03AB,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0314,
        composed: 0x1F59,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0340,
        composed: 0x1FEA,
    },
    ComposeEntry {
        base: 0x03A5,
        mark: 0x0341,
        composed: 0x038E,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0300,
        composed: 0x1FFA,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0301,
        composed: 0x038F,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0313,
        composed: 0x1F68,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0314,
        composed: 0x1F69,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0340,
        composed: 0x1FFA,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0341,
        composed: 0x038F,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0343,
        composed: 0x1F68,
    },
    ComposeEntry {
        base: 0x03A9,
        mark: 0x0345,
        composed: 0x1FFC,
    },
    ComposeEntry {
        base: 0x03AC,
        mark: 0x0345,
        composed: 0x1FB4,
    },
    ComposeEntry {
        base: 0x03AE,
        mark: 0x0345,
        composed: 0x1FC4,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0300,
        composed: 0x1F70,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0301,
        composed: 0x03AC,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0304,
        composed: 0x1FB1,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0306,
        composed: 0x1FB0,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0313,
        composed: 0x1F00,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0314,
        composed: 0x1F01,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0340,
        composed: 0x1F70,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0341,
        composed: 0x03AC,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0342,
        composed: 0x1FB6,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0343,
        composed: 0x1F00,
    },
    ComposeEntry {
        base: 0x03B1,
        mark: 0x0345,
        composed: 0x1FB3,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0300,
        composed: 0x1F72,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0301,
        composed: 0x03AD,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0313,
        composed: 0x1F10,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0314,
        composed: 0x1F11,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0340,
        composed: 0x1F72,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0341,
        composed: 0x03AD,
    },
    ComposeEntry {
        base: 0x03B5,
        mark: 0x0343,
        composed: 0x1F10,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0300,
        composed: 0x1F74,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0301,
        composed: 0x03AE,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0313,
        composed: 0x1F20,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0314,
        composed: 0x1F21,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0340,
        composed: 0x1F74,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0341,
        composed: 0x03AE,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0342,
        composed: 0x1FC6,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0343,
        composed: 0x1F20,
    },
    ComposeEntry {
        base: 0x03B7,
        mark: 0x0345,
        composed: 0x1FC3,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0300,
        composed: 0x1F76,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0301,
        composed: 0x03AF,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0304,
        composed: 0x1FD1,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0306,
        composed: 0x1FD0,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0308,
        composed: 0x03CA,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0313,
        composed: 0x1F30,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0314,
        composed: 0x1F31,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0340,
        composed: 0x1F76,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0341,
        composed: 0x03AF,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0342,
        composed: 0x1FD6,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0343,
        composed: 0x1F30,
    },
    ComposeEntry {
        base: 0x03B9,
        mark: 0x0344,
        composed: 0x0390,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0300,
        composed: 0x1F78,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0301,
        composed: 0x03CC,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0313,
        composed: 0x1F40,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0314,
        composed: 0x1F41,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0340,
        composed: 0x1F78,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0341,
        composed: 0x03CC,
    },
    ComposeEntry {
        base: 0x03BF,
        mark: 0x0343,
        composed: 0x1F40,
    },
    ComposeEntry {
        base: 0x03C1,
        mark: 0x0313,
        composed: 0x1FE4,
    },
    ComposeEntry {
        base: 0x03C1,
        mark: 0x0314,
        composed: 0x1FE5,
    },
    ComposeEntry {
        base: 0x03C1,
        mark: 0x0343,
        composed: 0x1FE4,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0300,
        composed: 0x1F7A,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0301,
        composed: 0x03CD,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0304,
        composed: 0x1FE1,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0306,
        composed: 0x1FE0,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0308,
        composed: 0x03CB,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0313,
        composed: 0x1F50,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0314,
        composed: 0x1F51,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0340,
        composed: 0x1F7A,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0341,
        composed: 0x03CD,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0342,
        composed: 0x1FE6,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0343,
        composed: 0x1F50,
    },
    ComposeEntry {
        base: 0x03C5,
        mark: 0x0344,
        composed: 0x03B0,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0300,
        composed: 0x1F7C,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0301,
        composed: 0x03CE,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0313,
        composed: 0x1F60,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0314,
        composed: 0x1F61,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0340,
        composed: 0x1F7C,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0341,
        composed: 0x03CE,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0342,
        composed: 0x1FF6,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0343,
        composed: 0x1F60,
    },
    ComposeEntry {
        base: 0x03C9,
        mark: 0x0345,
        composed: 0x1FF3,
    },
    ComposeEntry {
        base: 0x03CA,
        mark: 0x0300,
        composed: 0x1FD2,
    },
    ComposeEntry {
        base: 0x03CA,
        mark: 0x0301,
        composed: 0x0390,
    },
    ComposeEntry {
        base: 0x03CA,
        mark: 0x0340,
        composed: 0x1FD2,
    },
    ComposeEntry {
        base: 0x03CA,
        mark: 0x0341,
        composed: 0x0390,
    },
    ComposeEntry {
        base: 0x03CA,
        mark: 0x0342,
        composed: 0x1FD7,
    },
    ComposeEntry {
        base: 0x03CB,
        mark: 0x0300,
        composed: 0x1FE2,
    },
    ComposeEntry {
        base: 0x03CB,
        mark: 0x0301,
        composed: 0x03B0,
    },
    ComposeEntry {
        base: 0x03CB,
        mark: 0x0340,
        composed: 0x1FE2,
    },
    ComposeEntry {
        base: 0x03CB,
        mark: 0x0341,
        composed: 0x03B0,
    },
    ComposeEntry {
        base: 0x03CB,
        mark: 0x0342,
        composed: 0x1FE7,
    },
    ComposeEntry {
        base: 0x03CE,
        mark: 0x0345,
        composed: 0x1FF4,
    },
    ComposeEntry {
        base: 0x03D2,
        mark: 0x0301,
        composed: 0x03D3,
    },
    ComposeEntry {
        base: 0x03D2,
        mark: 0x0308,
        composed: 0x03D4,
    },
    ComposeEntry {
        base: 0x03D2,
        mark: 0x0341,
        composed: 0x03D3,
    },
    ComposeEntry {
        base: 0x0406,
        mark: 0x0308,
        composed: 0x0407,
    },
    ComposeEntry {
        base: 0x0410,
        mark: 0x0306,
        composed: 0x04D0,
    },
    ComposeEntry {
        base: 0x0410,
        mark: 0x0308,
        composed: 0x04D2,
    },
    ComposeEntry {
        base: 0x0413,
        mark: 0x0301,
        composed: 0x0403,
    },
    ComposeEntry {
        base: 0x0413,
        mark: 0x0341,
        composed: 0x0403,
    },
    ComposeEntry {
        base: 0x0415,
        mark: 0x0300,
        composed: 0x0400,
    },
    ComposeEntry {
        base: 0x0415,
        mark: 0x0306,
        composed: 0x04D6,
    },
    ComposeEntry {
        base: 0x0415,
        mark: 0x0308,
        composed: 0x0401,
    },
    ComposeEntry {
        base: 0x0415,
        mark: 0x0340,
        composed: 0x0400,
    },
    ComposeEntry {
        base: 0x0416,
        mark: 0x0306,
        composed: 0x04C1,
    },
    ComposeEntry {
        base: 0x0416,
        mark: 0x0308,
        composed: 0x04DC,
    },
    ComposeEntry {
        base: 0x0417,
        mark: 0x0308,
        composed: 0x04DE,
    },
    ComposeEntry {
        base: 0x0418,
        mark: 0x0300,
        composed: 0x040D,
    },
    ComposeEntry {
        base: 0x0418,
        mark: 0x0304,
        composed: 0x04E2,
    },
    ComposeEntry {
        base: 0x0418,
        mark: 0x0306,
        composed: 0x0419,
    },
    ComposeEntry {
        base: 0x0418,
        mark: 0x0308,
        composed: 0x04E4,
    },
    ComposeEntry {
        base: 0x0418,
        mark: 0x0340,
        composed: 0x040D,
    },
    ComposeEntry {
        base: 0x041A,
        mark: 0x0301,
        composed: 0x040C,
    },
    ComposeEntry {
        base: 0x041A,
        mark: 0x0341,
        composed: 0x040C,
    },
    ComposeEntry {
        base: 0x041E,
        mark: 0x0308,
        composed: 0x04E6,
    },
    ComposeEntry {
        base: 0x0423,
        mark: 0x0304,
        composed: 0x04EE,
    },
    ComposeEntry {
        base: 0x0423,
        mark: 0x0306,
        composed: 0x040E,
    },
    ComposeEntry {
        base: 0x0423,
        mark: 0x0308,
        composed: 0x04F0,
    },
    ComposeEntry {
        base: 0x0423,
        mark: 0x030B,
        composed: 0x04F2,
    },
    ComposeEntry {
        base: 0x0427,
        mark: 0x0308,
        composed: 0x04F4,
    },
    ComposeEntry {
        base: 0x042B,
        mark: 0x0308,
        composed: 0x04F8,
    },
    ComposeEntry {
        base: 0x042D,
        mark: 0x0308,
        composed: 0x04EC,
    },
    ComposeEntry {
        base: 0x0430,
        mark: 0x0306,
        composed: 0x04D1,
    },
    ComposeEntry {
        base: 0x0430,
        mark: 0x0308,
        composed: 0x04D3,
    },
    ComposeEntry {
        base: 0x0433,
        mark: 0x0301,
        composed: 0x0453,
    },
    ComposeEntry {
        base: 0x0433,
        mark: 0x0341,
        composed: 0x0453,
    },
    ComposeEntry {
        base: 0x0435,
        mark: 0x0300,
        composed: 0x0450,
    },
    ComposeEntry {
        base: 0x0435,
        mark: 0x0306,
        composed: 0x04D7,
    },
    ComposeEntry {
        base: 0x0435,
        mark: 0x0308,
        composed: 0x0451,
    },
    ComposeEntry {
        base: 0x0435,
        mark: 0x0340,
        composed: 0x0450,
    },
    ComposeEntry {
        base: 0x0436,
        mark: 0x0306,
        composed: 0x04C2,
    },
    ComposeEntry {
        base: 0x0436,
        mark: 0x0308,
        composed: 0x04DD,
    },
    ComposeEntry {
        base: 0x0437,
        mark: 0x0308,
        composed: 0x04DF,
    },
    ComposeEntry {
        base: 0x0438,
        mark: 0x0300,
        composed: 0x045D,
    },
    ComposeEntry {
        base: 0x0438,
        mark: 0x0304,
        composed: 0x04E3,
    },
    ComposeEntry {
        base: 0x0438,
        mark: 0x0306,
        composed: 0x0439,
    },
    ComposeEntry {
        base: 0x0438,
        mark: 0x0308,
        composed: 0x04E5,
    },
    ComposeEntry {
        base: 0x0438,
        mark: 0x0340,
        composed: 0x045D,
    },
    ComposeEntry {
        base: 0x043A,
        mark: 0x0301,
        composed: 0x045C,
    },
    ComposeEntry {
        base: 0x043A,
        mark: 0x0341,
        composed: 0x045C,
    },
    ComposeEntry {
        base: 0x043E,
        mark: 0x0308,
        composed: 0x04E7,
    },
    ComposeEntry {
        base: 0x0443,
        mark: 0x0304,
        composed: 0x04EF,
    },
    ComposeEntry {
        base: 0x0443,
        mark: 0x0306,
        composed: 0x045E,
    },
    ComposeEntry {
        base: 0x0443,
        mark: 0x0308,
        composed: 0x04F1,
    },
    ComposeEntry {
        base: 0x0443,
        mark: 0x030B,
        composed: 0x04F3,
    },
    ComposeEntry {
        base: 0x0447,
        mark: 0x0308,
        composed: 0x04F5,
    },
    ComposeEntry {
        base: 0x044B,
        mark: 0x0308,
        composed: 0x04F9,
    },
    ComposeEntry {
        base: 0x044D,
        mark: 0x0308,
        composed: 0x04ED,
    },
    ComposeEntry {
        base: 0x0456,
        mark: 0x0308,
        composed: 0x0457,
    },
    ComposeEntry {
        base: 0x0474,
        mark: 0x030F,
        composed: 0x0476,
    },
    ComposeEntry {
        base: 0x0475,
        mark: 0x030F,
        composed: 0x0477,
    },
    ComposeEntry {
        base: 0x04D8,
        mark: 0x0308,
        composed: 0x04DA,
    },
    ComposeEntry {
        base: 0x04D9,
        mark: 0x0308,
        composed: 0x04DB,
    },
    ComposeEntry {
        base: 0x04E8,
        mark: 0x0308,
        composed: 0x04EA,
    },
    ComposeEntry {
        base: 0x04E9,
        mark: 0x0308,
        composed: 0x04EB,
    },
    ComposeEntry {
        base: 0x1E36,
        mark: 0x0304,
        composed: 0x1E38,
    },
    ComposeEntry {
        base: 0x1E37,
        mark: 0x0304,
        composed: 0x1E39,
    },
    ComposeEntry {
        base: 0x1E5A,
        mark: 0x0304,
        composed: 0x1E5C,
    },
    ComposeEntry {
        base: 0x1E5B,
        mark: 0x0304,
        composed: 0x1E5D,
    },
    ComposeEntry {
        base: 0x1E60,
        mark: 0x0323,
        composed: 0x1E68,
    },
    ComposeEntry {
        base: 0x1E61,
        mark: 0x0323,
        composed: 0x1E69,
    },
    ComposeEntry {
        base: 0x1E62,
        mark: 0x0307,
        composed: 0x1E68,
    },
    ComposeEntry {
        base: 0x1E63,
        mark: 0x0307,
        composed: 0x1E69,
    },
    ComposeEntry {
        base: 0x1EA0,
        mark: 0x0302,
        composed: 0x1EAC,
    },
    ComposeEntry {
        base: 0x1EA0,
        mark: 0x0306,
        composed: 0x1EB6,
    },
    ComposeEntry {
        base: 0x1EA1,
        mark: 0x0302,
        composed: 0x1EAD,
    },
    ComposeEntry {
        base: 0x1EA1,
        mark: 0x0306,
        composed: 0x1EB7,
    },
    ComposeEntry {
        base: 0x1EB8,
        mark: 0x0302,
        composed: 0x1EC6,
    },
    ComposeEntry {
        base: 0x1EB9,
        mark: 0x0302,
        composed: 0x1EC7,
    },
    ComposeEntry {
        base: 0x1ECC,
        mark: 0x0302,
        composed: 0x1ED8,
    },
    ComposeEntry {
        base: 0x1ECC,
        mark: 0x031B,
        composed: 0x1EE2,
    },
    ComposeEntry {
        base: 0x1ECD,
        mark: 0x0302,
        composed: 0x1ED9,
    },
    ComposeEntry {
        base: 0x1ECD,
        mark: 0x031B,
        composed: 0x1EE3,
    },
    ComposeEntry {
        base: 0x1ECE,
        mark: 0x031B,
        composed: 0x1EDE,
    },
    ComposeEntry {
        base: 0x1ECF,
        mark: 0x031B,
        composed: 0x1EDF,
    },
    ComposeEntry {
        base: 0x1EE4,
        mark: 0x031B,
        composed: 0x1EF0,
    },
    ComposeEntry {
        base: 0x1EE5,
        mark: 0x031B,
        composed: 0x1EF1,
    },
    ComposeEntry {
        base: 0x1EE6,
        mark: 0x031B,
        composed: 0x1EEC,
    },
    ComposeEntry {
        base: 0x1EE7,
        mark: 0x031B,
        composed: 0x1EED,
    },
];
