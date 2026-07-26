//! Dependency-free, safe SHA-256 used by persistence wire contracts.

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Strict 32-byte SHA-256 digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256([u8; 32]);

/// Error returned for a non-canonical digest string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sha256ParseError;

impl Display for Sha256ParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SHA-256 must be exactly 64 lowercase hexadecimal characters")
    }
}

impl std::error::Error for Sha256ParseError {}

impl Sha256 {
    /// Compute SHA-256 for an in-memory byte slice.
    #[must_use]
    pub fn digest(input: &[u8]) -> Self {
        let mut state = INITIAL_STATE;
        let mut chunks = input.chunks_exact(64);
        for chunk in &mut chunks {
            compress(&mut state, chunk);
        }
        finish(&mut state, chunks.remainder(), input.len());
        Self(state_to_bytes(&state))
    }

    /// Borrow the fixed digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn finish(state: &mut [u32; 8], remainder: &[u8], input_len: usize) {
    let block_count = if remainder.len() < 56 { 1 } else { 2 };
    let mut final_blocks = [0u8; 128];
    final_blocks[..remainder.len()].copy_from_slice(remainder);
    final_blocks[remainder.len()] = 0x80;
    let bit_len = u64::try_from(input_len).unwrap_or(u64::MAX).wrapping_mul(8);
    let end = block_count * 64;
    final_blocks[end - 8..end].copy_from_slice(&bit_len.to_be_bytes());
    for block in final_blocks[..end].chunks_exact(64) {
        compress(state, block);
    }
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut schedule = [0u32; 64];
    for (index, word) in schedule[..16].iter_mut().enumerate() {
        let offset = index * 4;
        *word = u32::from_be_bytes([
            block[offset],
            block[offset + 1],
            block[offset + 2],
            block[offset + 3],
        ]);
    }
    for index in 16..64 {
        let s0 = small_sigma0(schedule[index - 15]);
        let s1 = small_sigma1(schedule[index - 2]);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }
    apply_rounds(state, &schedule);
}

fn apply_rounds(state: &mut [u32; 8], schedule: &[u32; 64]) {
    let mut work = *state;
    for index in 0..64 {
        let t1 = work[7]
            .wrapping_add(big_sigma1(work[4]))
            .wrapping_add(choice(work[4], work[5], work[6]))
            .wrapping_add(ROUND_CONSTANTS[index])
            .wrapping_add(schedule[index]);
        let t2 = big_sigma0(work[0]).wrapping_add(majority(work[0], work[1], work[2]));
        work = [
            t1.wrapping_add(t2),
            work[0],
            work[1],
            work[2],
            work[3].wrapping_add(t1),
            work[4],
            work[5],
            work[6],
        ];
    }
    for (slot, value) in state.iter_mut().zip(work) {
        *slot = slot.wrapping_add(value);
    }
}

const fn choice(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

const fn majority(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

const fn big_sigma0(value: u32) -> u32 {
    value.rotate_right(2) ^ value.rotate_right(13) ^ value.rotate_right(22)
}

const fn big_sigma1(value: u32) -> u32 {
    value.rotate_right(6) ^ value.rotate_right(11) ^ value.rotate_right(25)
}

const fn small_sigma0(value: u32) -> u32 {
    value.rotate_right(7) ^ value.rotate_right(18) ^ (value >> 3)
}

const fn small_sigma1(value: u32) -> u32 {
    value.rotate_right(17) ^ value.rotate_right(19) ^ (value >> 10)
}

fn state_to_bytes(state: &[u32; 8]) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, value) in state.iter().enumerate() {
        let offset = index * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
    bytes
}

impl Display for Sha256 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for Sha256 {
    type Err = Sha256ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(Sha256ParseError);
        }
        let mut bytes = [0u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
        }
        Ok(Self(bytes))
    }
}

const fn hex_value(byte: u8) -> Result<u8, Sha256ParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(Sha256ParseError),
    }
}

impl Serialize for Sha256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}
