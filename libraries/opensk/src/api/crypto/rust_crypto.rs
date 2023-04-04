// Copyright 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This cryptography implementation is an alternative for our own library.
//!
//! You can use it with the `rust_crypto` feature. An example call to cargo test is in
//! `run_desktop_tests.sh`. It is currently impossible to use it with our version of TockOS due to
//! a compiler version imcompatibility.
//!
//! If you want to use OpenSK outside of Tock v1, maybe this is useful for you though!

use crate::api::crypto::hkdf256::Hkdf256;
use crate::api::crypto::hmac256::Hmac256;
use crate::api::crypto::sha256::Sha256;
use crate::api::crypto::{
    ecdh, ecdsa, Crypto, EC_FIELD_SIZE, EC_SIGNATURE_SIZE, HASH_SIZE, HMAC_KEY_SIZE,
    TRUNCATED_HMAC_SIZE,
};
use core::convert::TryFrom;
use hmac::Mac;
use p256::ecdh::EphemeralSecret;
use p256::ecdsa::signature::{SignatureEncoding, Signer, Verifier};
use p256::ecdsa::{SigningKey, VerifyingKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
// TODO: implement CryptoRngCore for our Rng instead
use rand_core::OsRng;
use rng256::Rng256;
use sha2::Digest;

pub struct SoftwareCrypto;
pub struct SoftwareEcdh;
pub struct SoftwareEcdsa;

impl Crypto for SoftwareCrypto {
    type Ecdh = SoftwareEcdh;
    type Ecdsa = SoftwareEcdsa;
    type Sha256 = SoftwareSha256;
    type Hmac256 = SoftwareHmac256;
    type Hkdf256 = SoftwareHkdf256;
}

impl ecdh::Ecdh for SoftwareEcdh {
    type SecretKey = SoftwareEcdhSecretKey;
    type PublicKey = SoftwareEcdhPublicKey;
    type SharedSecret = SoftwareEcdhSharedSecret;
}

pub struct SoftwareEcdhSecretKey {
    ephemeral_secret: EphemeralSecret,
}

impl ecdh::SecretKey for SoftwareEcdhSecretKey {
    type PublicKey = SoftwareEcdhPublicKey;
    type SharedSecret = SoftwareEcdhSharedSecret;

    fn random(_rng: &mut impl Rng256) -> Self {
        let ephemeral_secret = EphemeralSecret::random(&mut OsRng);
        Self { ephemeral_secret }
    }

    fn public_key(&self) -> Self::PublicKey {
        let public_key = self.ephemeral_secret.public_key();
        SoftwareEcdhPublicKey { public_key }
    }

    fn diffie_hellman(&self, public_key: &SoftwareEcdhPublicKey) -> Self::SharedSecret {
        let shared_secret = self.ephemeral_secret.diffie_hellman(&public_key.public_key);
        SoftwareEcdhSharedSecret { shared_secret }
    }
}

pub struct SoftwareEcdhPublicKey {
    public_key: p256::PublicKey,
}

impl ecdh::PublicKey for SoftwareEcdhPublicKey {
    fn from_coordinates(x: &[u8; EC_FIELD_SIZE], y: &[u8; EC_FIELD_SIZE]) -> Option<Self> {
        let encoded_point: p256::EncodedPoint =
            p256::EncodedPoint::from_affine_coordinates(x.into(), y.into(), false);
        let public_key = p256::PublicKey::from_sec1_bytes(encoded_point.as_bytes()).ok()?;
        Some(Self { public_key })
    }

    fn to_coordinates(&self, x: &mut [u8; EC_FIELD_SIZE], y: &mut [u8; EC_FIELD_SIZE]) {
        let point = self.public_key.to_encoded_point(false);
        x.copy_from_slice(point.x().unwrap());
        y.copy_from_slice(point.y().unwrap());
    }
}

pub struct SoftwareEcdhSharedSecret {
    shared_secret: p256::ecdh::SharedSecret,
}

impl ecdh::SharedSecret for SoftwareEcdhSharedSecret {
    fn raw_secret_bytes(&self) -> [u8; EC_FIELD_SIZE] {
        let mut bytes = [0; EC_FIELD_SIZE];
        bytes.copy_from_slice(self.shared_secret.raw_secret_bytes().as_slice());
        bytes
    }
}

impl ecdsa::Ecdsa for SoftwareEcdsa {
    type SecretKey = SoftwareEcdsaSecretKey;
    type PublicKey = SoftwareEcdsaPublicKey;
    type Signature = SoftwareEcdsaSignature;
}

pub struct SoftwareEcdsaSecretKey {
    signing_key: SigningKey,
}

impl ecdsa::SecretKey for SoftwareEcdsaSecretKey {
    type PublicKey = SoftwareEcdsaPublicKey;
    type Signature = SoftwareEcdsaSignature;

    fn random(_rng: &mut impl Rng256) -> Self {
        let signing_key = SigningKey::random(&mut OsRng);
        SoftwareEcdsaSecretKey { signing_key }
    }

    fn from_slice(bytes: &[u8; EC_FIELD_SIZE]) -> Option<Self> {
        let signing_key = SigningKey::from_slice(bytes).ok()?;
        Some(SoftwareEcdsaSecretKey { signing_key })
    }

    fn public_key(&self) -> Self::PublicKey {
        let verifying_key = VerifyingKey::from(&self.signing_key);
        SoftwareEcdsaPublicKey { verifying_key }
    }

    fn sign(&self, message: &[u8]) -> Self::Signature {
        let signature = self.signing_key.sign(message);
        SoftwareEcdsaSignature { signature }
    }

    fn to_slice(&self, bytes: &mut [u8; EC_FIELD_SIZE]) {
        bytes.copy_from_slice(&self.signing_key.to_bytes());
    }
}

pub struct SoftwareEcdsaPublicKey {
    verifying_key: VerifyingKey,
}

impl ecdsa::PublicKey for SoftwareEcdsaPublicKey {
    type Signature = SoftwareEcdsaSignature;

    fn from_coordinates(x: &[u8; EC_FIELD_SIZE], y: &[u8; EC_FIELD_SIZE]) -> Option<Self> {
        let encoded_point: p256::EncodedPoint =
            p256::EncodedPoint::from_affine_coordinates(x.into(), y.into(), false);
        let verifying_key = VerifyingKey::from_encoded_point(&encoded_point).ok()?;
        Some(SoftwareEcdsaPublicKey { verifying_key })
    }

    fn verify(&self, message: &[u8], signature: &Self::Signature) -> bool {
        self.verifying_key
            .verify(message, &signature.signature)
            .is_ok()
    }

    fn to_coordinates(&self, x: &mut [u8; EC_FIELD_SIZE], y: &mut [u8; EC_FIELD_SIZE]) {
        let point = self.verifying_key.to_encoded_point(false);
        x.copy_from_slice(point.x().unwrap());
        y.copy_from_slice(point.y().unwrap());
    }
}

pub struct SoftwareEcdsaSignature {
    signature: p256::ecdsa::Signature,
}

impl ecdsa::Signature for SoftwareEcdsaSignature {
    fn from_slice(bytes: &[u8; EC_SIGNATURE_SIZE]) -> Option<Self> {
        // Assumes EC_SIGNATURE_SIZE == 2 * EC_FIELD_SIZE
        let r = &bytes[..EC_FIELD_SIZE];
        let s = &bytes[EC_FIELD_SIZE..];
        let r = p256::NonZeroScalar::try_from(r).ok()?;
        let s = p256::NonZeroScalar::try_from(s).ok()?;
        let r = p256::FieldBytes::from(r);
        let s = p256::FieldBytes::from(s);
        let signature = p256::ecdsa::Signature::from_scalars(r, s).ok()?;
        Some(SoftwareEcdsaSignature { signature })
    }

    fn to_der(&self) -> Vec<u8> {
        self.signature.to_der().to_vec()
    }
}

pub struct SoftwareSha256 {
    hasher: sha2::Sha256,
}

impl Sha256 for SoftwareSha256 {
    fn digest(data: &[u8]) -> [u8; HASH_SIZE] {
        sha2::Sha256::digest(data).into()
    }

    fn new() -> Self {
        let hasher = sha2::Sha256::new();
        Self { hasher }
    }

    /// Digest the next part of the message to hash.
    fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Finalizes the hashing process, returns the hash value.
    fn finalize(self) -> [u8; HASH_SIZE] {
        self.hasher.finalize().into()
    }
}

pub struct SoftwareHmac256;

impl Hmac256 for SoftwareHmac256 {
    fn mac(key: &[u8; HMAC_KEY_SIZE], data: &[u8]) -> [u8; HASH_SIZE] {
        let mut hmac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
        hmac.update(data);
        hmac.finalize().into_bytes().into()
    }

    fn verify(key: &[u8; HMAC_KEY_SIZE], data: &[u8], mac: &[u8; HASH_SIZE]) -> bool {
        let mut hmac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
        hmac.update(data);
        hmac.verify_slice(mac).is_ok()
    }

    fn verify_truncated_left(
        key: &[u8; HMAC_KEY_SIZE],
        data: &[u8],
        mac: &[u8; TRUNCATED_HMAC_SIZE],
    ) -> bool {
        let mut hmac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
        hmac.update(data);
        hmac.verify_truncated_left(mac).is_ok()
    }
}

pub struct SoftwareHkdf256;

impl Hkdf256 for SoftwareHkdf256 {
    fn hkdf_empty_salt_256(ikm: &[u8], info: &[u8]) -> [u8; HASH_SIZE] {
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[0; HASH_SIZE]), ikm);
        let mut okm = [0u8; HASH_SIZE];
        hk.expand(info, &mut okm).unwrap();
        okm
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::crypto::ecdh::{
        PublicKey as EcdhPublicKey, SecretKey as EcdhSecretKey, SharedSecret,
    };
    use crate::api::crypto::ecdsa::{PublicKey as EcdsaPublicKey, SecretKey as EcdsaSecretKey};
    use crate::env::test::TestEnv;

    #[test]
    fn test_shared_secret_symmetric() {
        let mut env = TestEnv::default();
        let private1 = SoftwareEcdhSecretKey::random(env.rng());
        let private2 = SoftwareEcdhSecretKey::random(env.rng());
        let pub1 = private1.public_key();
        let pub2 = private2.public_key();
        let shared1 = private1.diffie_hellman(&pub2);
        let shared2 = private2.diffie_hellman(&pub1);
        assert_eq!(shared1.raw_secret_bytes(), shared2.raw_secret_bytes());
    }

    #[test]
    fn test_ecdh_public_key_from_to_bytes() {
        let mut env = TestEnv::default();
        let first_key = SoftwareEcdhSecretKey::random(env.rng());
        let first_public = first_key.public_key();
        let mut x = [0; EC_FIELD_SIZE];
        let mut y = [0; EC_FIELD_SIZE];
        first_public.to_coordinates(&mut x, &mut y);
        let new_public = SoftwareEcdhPublicKey::from_coordinates(&x, &y).unwrap();
        let mut new_x = [0; EC_FIELD_SIZE];
        let mut new_y = [0; EC_FIELD_SIZE];
        new_public.to_coordinates(&mut new_x, &mut new_y);
        assert_eq!(x, new_x);
        assert_eq!(y, new_y);
    }

    #[test]
    fn test_sign_verify() {
        let mut env = TestEnv::default();
        let private_key = SoftwareEcdsaSecretKey::random(env.rng());
        let public_key = private_key.public_key();
        let message = [0x12, 0x34, 0x56, 0x78];
        let signature = private_key.sign(&message);
        assert!(public_key.verify(&message, &signature));
    }

    #[test]
    fn test_ecdsa_secret_key_from_to_bytes() {
        let mut env = TestEnv::default();
        let first_key = SoftwareEcdsaSecretKey::random(env.rng());
        let mut key_bytes = [0; EC_FIELD_SIZE];
        first_key.to_slice(&mut key_bytes);
        let second_key = SoftwareEcdsaSecretKey::from_slice(&key_bytes).unwrap();
        let mut new_bytes = [0; EC_FIELD_SIZE];
        second_key.to_slice(&mut new_bytes);
        assert_eq!(key_bytes, new_bytes);
    }

    #[test]
    fn test_sha256_hash_matches() {
        let data = [0x55; 16];
        let mut hasher = SoftwareSha256::new();
        hasher.update(&data);
        assert_eq!(SoftwareSha256::digest(&data), hasher.finalize());
    }

    #[test]
    fn test_hmac256_verifies() {
        let key = [0xAA; HMAC_KEY_SIZE];
        let data = [0x55; 16];
        let mac = SoftwareHmac256::mac(&key, &data);
        assert!(SoftwareHmac256::verify(&key, &data, &mac));
        let truncated_mac =
            <&[u8; TRUNCATED_HMAC_SIZE]>::try_from(&mac[..TRUNCATED_HMAC_SIZE]).unwrap();
        assert!(SoftwareHmac256::verify_truncated_left(
            &key,
            &data,
            &truncated_mac
        ));
    }

    #[test]
    fn test_hkdf_empty_salt_256_vector() {
        let okm = [
            0xf9, 0xbe, 0x72, 0x11, 0x6c, 0xb9, 0x7f, 0x41, 0x82, 0x82, 0x10, 0x28, 0x9c, 0xaa,
            0xfe, 0xab, 0xde, 0x1f, 0x3d, 0xfb, 0x97, 0x23, 0xbf, 0x43, 0x53, 0x8a, 0xb1, 0x8f,
            0x36, 0x66, 0x78, 0x3a,
        ];
        assert_eq!(&SoftwareHkdf256::hkdf_empty_salt_256(b"0", &[0]), &okm);
    }
}
