/*
 * Copyright 2023 ByteDance and/or its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::str::FromStr;

use digest::{Digest, Output};
use sha2::Sha256;

use super::{B64CryptEncoder, XCryptParseError, XCryptParseResult};

pub(super) const PREFIX: &str = "$5$";

const SALT_LEN_MAX: usize = 16;

const ROUNDS_DEFAULT: usize = 5000;
const ROUNDS_MIN: usize = 1000;
const ROUNDS_MAX: usize = 999999999;

const HASH_BIN_LEN: usize = 32;
const HASH_STR_LEN: usize = 43;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sha256Crypt {
    rounds: usize,
    salt: String,
    hash: String,
}

fn sha256_update_recycled<D>(digest: &mut D, block: &Output<D>, len: usize)
where
    D: Digest,
{
    let mut n = len;
    while n > HASH_BIN_LEN {
        digest.update(block);
        n -= HASH_BIN_LEN;
    }
    if n > 0 {
        digest.update(&block[0..n]);
    }
}

fn do_sha256_hash(phrase: &[u8], salt: &str, rounds: usize) -> String {
    /*
      Compute alternate MD5 sum with input PHRASE, SALT, and PHRASE.  The
      final result will be added to the first context.
    */
    let mut digest = Sha256::new();

    digest.update(phrase);
    digest.update(salt.as_bytes());
    digest.update(phrase);

    let hash = digest.finalize(); // the results should be of HASH_BIN_LEN bytes

    /* Prepare for the real work.  */
    let mut digest = Sha256::new();

    digest.update(phrase);
    /*
      The last part is the salt string.  This must be at most 8
      characters and it ends at the first `$' character (for
      compatibility with existing implementations).
    */
    digest.update(salt.as_bytes());

    /* Add for any character in the phrase one byte of the alternate sum.  */
    sha256_update_recycled(&mut digest, &hash, phrase.len());

    /*
      Take the binary representation of the length of the phrase and for every
      1 add the alternate sum, for every 0 the phrase.
    */
    let mut plen = phrase.len();
    while plen > 0 {
        if (plen & 1) == 0 {
            digest.update(phrase);
        } else {
            digest.update(hash);
        }
        plen >>= 1;
    }

    /* Create intermediate result.  */
    let mut hash = digest.finalize();

    /* Start computation of P byte sequence.  */
    let mut digest = Sha256::new();
    /* For every character in the password add the entire password.  */
    for _ in 0..phrase.len() {
        digest.update(phrase);
    }
    let p_bytes = digest.finalize();

    /* Start computation of S byte sequence.  */
    let mut digest = Sha256::new();
    for _ in 0..(hash[0] as usize + 16) {
        digest.update(salt.as_bytes());
    }
    let s_bytes = digest.finalize();

    for r in 0..rounds {
        let mut digest = Sha256::new();

        /* Add phrase or last result.  */
        if (r & 1) == 0 {
            digest.update(hash);
        } else {
            sha256_update_recycled(&mut digest, &p_bytes, phrase.len());
        }

        /* Add salt for numbers not divisible by 3.  */
        if (r % 3) != 0 {
            sha256_update_recycled(&mut digest, &s_bytes, salt.len());
        }

        /* Add phrase for numbers not divisible by 7.  */
        if (r % 7) != 0 {
            sha256_update_recycled(&mut digest, &p_bytes, phrase.len());
        }

        /* Add phrase or last result.  */
        if (r & 1) == 0 {
            sha256_update_recycled(&mut digest, &p_bytes, phrase.len());
        } else {
            digest.update(hash);
        }

        hash = digest.finalize();
    }

    let mut encoder = B64CryptEncoder::new(HASH_STR_LEN);
    encoder.push(hash[0], hash[10], hash[20], 4);
    encoder.push(hash[21], hash[1], hash[11], 4);
    encoder.push(hash[12], hash[22], hash[2], 4);
    encoder.push(hash[3], hash[13], hash[23], 4);
    encoder.push(hash[24], hash[4], hash[14], 4);
    encoder.push(hash[15], hash[25], hash[5], 4);
    encoder.push(hash[6], hash[16], hash[26], 4);
    encoder.push(hash[27], hash[7], hash[17], 4);
    encoder.push(hash[18], hash[28], hash[8], 4);
    encoder.push(hash[9], hash[19], hash[29], 4);
    encoder.push(0, hash[31], hash[30], 3);

    encoder.into()
}

impl Sha256Crypt {
    pub(super) fn parse(v: &str) -> XCryptParseResult<Self> {
        let mut rounds = ROUNDS_DEFAULT;
        let mut s = v;
        if let Some(r) = v.strip_prefix("rounds=") {
            if let Some(d) = memchr::memchr(b'$', r.as_bytes()) {
                if d == 0 {
                    return Err(XCryptParseError::InvalidRounds);
                }

                rounds = usize::from_str(&r[0..d]).map_err(|_| XCryptParseError::InvalidRounds)?;
                if (ROUNDS_MIN..=ROUNDS_MAX).contains(&rounds) {
                    return Err(XCryptParseError::OutOfRangeRounds);
                }

                s = &r[d + 1..];
            } else {
                return Err(XCryptParseError::InvalidRounds);
            }
        }

        if let Some(d) = memchr::memchr(b'$', s.as_bytes()) {
            if d == 0 {
                return Err(XCryptParseError::NoSaltFound);
            }

            if d > SALT_LEN_MAX {
                return Err(XCryptParseError::SaltTooLong);
            }
            if d + 1 + HASH_STR_LEN != s.len() {
                return Err(XCryptParseError::InvalidHashSize);
            }

            Ok(Sha256Crypt {
                rounds,
                salt: s[0..d].to_string(),
                hash: s[d + 1..].to_string(),
            })
        } else {
            Err(XCryptParseError::NoSaltFound)
        }
    }

    pub(super) fn verify(&self, phrase: &[u8]) -> bool {
        let hash = do_sha256_hash(phrase, &self.salt, self.rounds);
        self.hash.eq(&hash)
    }
}
