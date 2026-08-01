//! Provenance verification for the multiplexer binary (issue #540).
//!
//! CI downloads a pinned psmux archive and refuses it unless the SHA256
//! matches. User machines verified nothing beyond parsing `-V`, so whatever
//! `psmux.exe` was first on `PATH` was trusted.
//!
//! The archive digest cannot be applied to a binary on `PATH`: it covers the
//! zip, not the extracted executable, and a user's psmux may have arrived by
//! another route. Claiming otherwise would be a check that looks like
//! verification and is not. What jefe can state honestly is narrower -- it
//! records the fingerprint of the binary it qualified, refuses one it has never
//! qualified, and detects the binary changing underneath a running process.
//!
//! SHA-256 is implemented here rather than added as a dependency, and is
//! checked against the published NIST vectors in
//! `tests/runtime_provenance.rs`.

use std::fmt::Write as _;
use std::path::Path;

/// The psmux archive digest CI pins, recorded so the provenance of the
/// qualified release is stated rather than implied.
pub const PINNED_PSMUX_ARCHIVE_SHA256: &str =
    "60ff7b236f64184921cef3c1ff2611aa5a36fcc7ed8e2a58e968b8ded57f6028";

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

/// SHA-256 of `bytes`, lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut state = INITIAL_STATE;

    // Pad: 0x80, zeroes, then the bit length as big-endian u64.
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        compress(&mut state, block);
    }

    state.iter().fold(String::new(), |mut hex, word| {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{word:08x}");
        hex
    })
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut schedule = [0u32; 64];
    for (index, slot) in schedule.iter_mut().take(16).enumerate() {
        let start = index * 4;
        *slot = u32::from_be_bytes([
            block[start],
            block[start + 1],
            block[start + 2],
            block[start + 3],
        ]);
    }
    for index in 16..64 {
        let s0 = schedule[index - 15].rotate_right(7)
            ^ schedule[index - 15].rotate_right(18)
            ^ (schedule[index - 15] >> 3);
        let s1 = schedule[index - 2].rotate_right(17)
            ^ schedule[index - 2].rotate_right(19)
            ^ (schedule[index - 2] >> 10);
        schedule[index] = schedule[index - 16]
            .wrapping_add(s0)
            .wrapping_add(schedule[index - 7])
            .wrapping_add(s1);
    }

    // Named for the FIPS 180-4 working variables a..h, kept as an array so the
    // single-letter names the specification uses do not leak into scope.
    let mut work = *state;

    for (word, constant) in schedule.iter().zip(ROUND_CONSTANTS.iter()) {
        let s1 = work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
        let choose = (work[4] & work[5]) ^ ((!work[4]) & work[6]);
        let temp1 = work[7]
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(*constant)
            .wrapping_add(*word);
        let s0 = work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
        let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
        let temp2 = s0.wrapping_add(majority);

        work[7] = work[6];
        work[6] = work[5];
        work[5] = work[4];
        work[4] = work[3].wrapping_add(temp1);
        work[3] = work[2];
        work[2] = work[1];
        work[1] = work[0];
        work[0] = temp1.wrapping_add(temp2);
    }

    for (slot, value) in state.iter_mut().zip(work) {
        *slot = slot.wrapping_add(value);
    }
}

/// What jefe observed about a multiplexer binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryFingerprint {
    path: String,
    len: u64,
    sha256: String,
}

impl BinaryFingerprint {
    /// Record a fingerprint.
    #[must_use]
    pub fn new(path: impl Into<String>, len: u64, sha256: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            len,
            sha256: sha256.into(),
        }
    }

    /// Fingerprint the binary at `path` by reading it.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the binary cannot be read.
    pub fn measure(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self {
            path: path.display().to_string(),
            len: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
        })
    }

    /// The recorded digest.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The path observed.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The length observed.
    #[must_use]
    pub const fn len_bytes(&self) -> u64 {
        self.len
    }

    /// Whether the binary changed since this fingerprint was taken.
    ///
    /// The digest decides, so a replacement of identical length is still
    /// caught; the length is carried only to make the diagnostic legible.
    #[must_use]
    pub fn detect_change(&self, current: &Self) -> ProvenanceVerdict {
        if self.sha256 == current.sha256 {
            return ProvenanceVerdict::Qualified;
        }
        ProvenanceVerdict::Changed {
            diagnostic: format!(
                "the multiplexer binary changed while jefe was running.\n  \
                 path:      {}\n  \
                 qualified: {} ({} bytes)\n  \
                 now:       {} ({} bytes)\n\
                 jefe qualified the earlier binary and cannot vouch for this one. \
                 Restart jefe to re-qualify, or restore the binary it started with.",
                current.path, self.sha256, self.len, current.sha256, current.len,
            ),
        }
    }
}

/// The outcome of a provenance check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceVerdict {
    /// The binary is one jefe qualified.
    Qualified,
    /// The binary is not one jefe has qualified.
    Unqualified {
        /// Operator-facing diagnostic naming path, digest and remedy.
        diagnostic: String,
    },
    /// The binary changed underneath a running jefe.
    Changed {
        /// Operator-facing diagnostic showing both digests.
        diagnostic: String,
    },
}

/// The set of binary digests jefe has qualified.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProvenanceManifest {
    qualified: Vec<String>,
}

impl ProvenanceManifest {
    /// Build a manifest from known-qualified digests.
    #[must_use]
    pub fn with_qualified(digests: impl IntoIterator<Item = String>) -> Self {
        Self {
            qualified: digests.into_iter().collect(),
        }
    }

    /// Whether any digest is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.qualified.is_empty()
    }

    /// Check a fingerprint against the manifest.
    #[must_use]
    pub fn verify(&self, fingerprint: &BinaryFingerprint) -> ProvenanceVerdict {
        if self
            .qualified
            .iter()
            .any(|digest| digest == &fingerprint.sha256)
        {
            return ProvenanceVerdict::Qualified;
        }
        ProvenanceVerdict::Unqualified {
            diagnostic: format!(
                "the multiplexer on PATH is not one jefe has qualified.\n  \
                 path:   {}\n  \
                 sha256: {}\n  \
                 bytes:  {}\n\
                 jefe verifies the release archive {PINNED_PSMUX_ARCHIVE_SHA256} in CI, but \
                 cannot tell where this executable came from. Install the pinned release, or \
                 point JEFE_PSMUX_BIN at a binary you trust and record this digest as \
                 qualified.",
                fingerprint.path, fingerprint.sha256, fingerprint.len,
            ),
        }
    }
}
