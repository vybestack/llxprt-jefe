//! Known-answer vectors and streaming equivalence for [`Sha256`]
//! (issue #389 CW-09, scope-ledger item S1 for acceptance row A7).

use super::*;

/// NIST FIPS 180-4 / RFC 6234 published vectors.
const KNOWN_ANSWERS: [(&[u8], &str); 5] = [
    (
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        b"abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ),
    (
        b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    ),
    (
        b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
    ),
    (
        b"The quick brown fox jumps over the lazy dog",
        "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
    ),
];

fn expected(text: &str) -> Sha256 {
    text.parse::<Sha256>()
        .unwrap_or_else(|error| panic!("{text} must parse: {error}"))
}

#[test]
fn one_shot_digest_matches_every_known_answer() {
    for (input, answer) in KNOWN_ANSWERS {
        assert_eq!(
            Sha256::digest(input),
            expected(answer),
            "one-shot digest must match the published vector for {input:?}"
        );
    }
}

#[test]
fn streaming_digest_matches_every_known_answer() {
    for (input, answer) in KNOWN_ANSWERS {
        let mut hasher = Sha256Hasher::new();
        hasher.update(input);
        assert_eq!(
            hasher.finalize(),
            expected(answer),
            "streamed digest must match the published vector for {input:?}"
        );
    }
}

#[test]
fn an_unfed_hasher_digests_the_empty_input() {
    assert_eq!(Sha256Hasher::new().finalize(), Sha256::digest(b""));
}

#[test]
fn streaming_is_independent_of_chunk_boundaries() {
    // 1000 bytes crosses many 64-byte compression blocks, and the chunk sizes
    // straddle the block size, the 56-byte padding threshold, and both.
    let input: Vec<u8> = (0..1000u32).map(|index| (index % 251) as u8).collect();
    let one_shot = Sha256::digest(&input);
    for chunk_size in [1, 7, 55, 56, 57, 63, 64, 65, 127, 128, 129, 999, 1000] {
        let mut hasher = Sha256Hasher::new();
        for chunk in input.chunks(chunk_size) {
            hasher.update(chunk);
        }
        assert_eq!(
            hasher.finalize(),
            one_shot,
            "a {chunk_size}-byte feed must equal the one-shot digest"
        );
    }
}

#[test]
fn empty_updates_do_not_change_the_digest() {
    let mut hasher = Sha256Hasher::new();
    hasher.update(b"");
    hasher.update(b"ab");
    hasher.update(b"");
    hasher.update(b"c");
    hasher.update(b"");
    assert_eq!(hasher.finalize(), Sha256::digest(b"abc"));
}

#[test]
fn streaming_matches_one_shot_across_every_length_through_two_blocks() {
    let input: Vec<u8> = (0..=200u32).map(|index| (index % 251) as u8).collect();
    for length in 0..input.len() {
        let slice = &input[..length];
        let mut hasher = Sha256Hasher::new();
        for chunk in slice.chunks(17) {
            hasher.update(chunk);
        }
        assert_eq!(
            hasher.finalize(),
            Sha256::digest(slice),
            "length {length} must agree between streaming and one-shot"
        );
    }
}

#[test]
fn the_hasher_reports_how_many_bytes_it_consumed() {
    let mut hasher = Sha256Hasher::new();
    assert_eq!(hasher.len(), 0);
    hasher.update(&[0u8; 100]);
    assert_eq!(hasher.len(), 100);
    hasher.update(&[0u8; 28]);
    assert_eq!(hasher.len(), 128);
}

#[test]
fn a_digest_round_trips_through_its_canonical_text() {
    for (input, answer) in KNOWN_ANSWERS {
        let digest = Sha256::digest(input);
        assert_eq!(digest.to_string(), answer);
        assert_eq!(digest.to_string().parse::<Sha256>(), Ok(digest));
    }
}
