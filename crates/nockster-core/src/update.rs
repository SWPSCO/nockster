//! Signed firmware update manifest primitives.
//!
//! This module intentionally handles only release/update authentication
//! metadata. It is separate from transaction signing code.

use crate::UpdateStatus;
use heapless::String;
use k256::ecdsa::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const UPDATE_MANIFEST_VERSION: u8 = 1;
pub const UPDATE_SIGNATURE_SCHEME: &str = "secp256k1-ecdsa-sha256-prehash-v1";
pub const UPDATE_SIGNATURE_DOMAIN: &[u8] = b"nockster-fw-update-v1";
pub const UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47: &str = "esp32s3-touch-lcd-1.47";
pub const UPDATE_BUILD_PROFILE_DEV: &str = "dev";
pub const UPDATE_BUILD_PROFILE_CHIP_SECURITY: &str = "chip-security";
pub const UPDATE_BUILD_PROFILE_PRODUCTION: &str = "production";
pub const MAX_UPDATE_IMAGE_SIZE: u32 = 4 * 1024 * 1024;
pub const MAX_UPDATE_CHUNK_LEN: usize = 512;

pub const MAX_HARDWARE_TARGET_LEN: usize = 32;
pub const MAX_BUILD_PROFILE_LEN: usize = 16;
pub const MAX_GIT_COMMIT_LEN: usize = 40;
pub const MAX_TX_TYPES_REV_LEN: usize = 40;
pub const MAX_UPDATE_MANIFEST_POSTCARD_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub manifest_version: u8,
    pub release_version: u32,
    pub image_size: u32,
    pub image_sha256: [u8; 32],
    pub signing_pubkey_sha256: [u8; 32],
    pub hardware_target: String<MAX_HARDWARE_TARGET_LEN>,
    pub build_profile: String<MAX_BUILD_PROFILE_LEN>,
    pub protocol_v: u8,
    pub git_commit: String<MAX_GIT_COMMIT_LEN>,
    pub tx_types_rev: String<MAX_TX_TYPES_REV_LEN>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateManifestError {
    StringTooLong,
    ImageTooLarge,
    Encode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSignatureError {
    Manifest(UpdateManifestError),
    BadPublicKey,
    BadSignature,
    UntrustedPublicKey,
    RollbackVersion,
    ImageHashMismatch,
    VerifyFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateManifestPolicy<'a> {
    pub current_release_version: u32,
    pub hardware_target: &'a str,
    pub current_build_profile: &'a str,
    pub protocol_v: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateManifestPolicyError {
    UnsupportedManifest,
    RollbackVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateImageStreamError {
    ChunkTooLarge,
    OffsetMismatch,
    Overflow,
    IncompleteImage,
    ImageHashMismatch,
}

#[derive(Clone)]
pub struct UpdateImageVerifier {
    manifest: UpdateManifest,
    hasher: Sha256,
    received: u32,
}

impl UpdateManifest {
    pub fn new(
        release_version: u32,
        image_size: u32,
        image_sha256: [u8; 32],
        signing_pubkey_sha256: [u8; 32],
        hardware_target: &str,
        build_profile: &str,
        protocol_v: u8,
        git_commit: &str,
        tx_types_rev: &str,
    ) -> Result<Self, UpdateManifestError> {
        if image_size > MAX_UPDATE_IMAGE_SIZE {
            return Err(UpdateManifestError::ImageTooLarge);
        }

        Ok(Self {
            manifest_version: UPDATE_MANIFEST_VERSION,
            release_version,
            image_size,
            image_sha256,
            signing_pubkey_sha256,
            hardware_target: heapless_string(hardware_target)?,
            build_profile: heapless_string(build_profile)?,
            protocol_v,
            git_commit: heapless_string(git_commit)?,
            tx_types_rev: heapless_string(tx_types_rev)?,
        })
    }
}

impl UpdateImageVerifier {
    pub fn new(manifest: UpdateManifest) -> Self {
        Self {
            manifest,
            hasher: Sha256::new(),
            received: 0,
        }
    }

    pub fn append_chunk(
        &mut self,
        offset: u32,
        chunk: &[u8],
    ) -> Result<UpdateStatus, UpdateImageStreamError> {
        if chunk.len() > MAX_UPDATE_CHUNK_LEN {
            return Err(UpdateImageStreamError::ChunkTooLarge);
        }
        if self.received != offset {
            return Err(UpdateImageStreamError::OffsetMismatch);
        }

        let next = self
            .received
            .checked_add(chunk.len() as u32)
            .ok_or(UpdateImageStreamError::Overflow)?;
        if next > self.manifest.image_size {
            return Err(UpdateImageStreamError::Overflow);
        }

        self.hasher.update(chunk);
        self.received = next;
        Ok(self.status(false))
    }

    pub fn finish(self) -> Result<UpdateStatus, UpdateImageStreamError> {
        if self.received != self.manifest.image_size {
            return Err(UpdateImageStreamError::IncompleteImage);
        }

        let digest = self.hasher.finalize();
        let mut image_sha256 = [0u8; 32];
        image_sha256.copy_from_slice(&digest);
        if image_sha256 != self.manifest.image_sha256 {
            return Err(UpdateImageStreamError::ImageHashMismatch);
        }

        Ok(UpdateStatus {
            active: false,
            manifest_verified: true,
            image_verified: true,
            release_version: self.manifest.release_version,
            bytes_received: self.received,
            image_size: self.manifest.image_size,
        })
    }

    pub fn status(&self, image_verified: bool) -> UpdateStatus {
        UpdateStatus {
            active: true,
            manifest_verified: true,
            image_verified,
            release_version: self.manifest.release_version,
            bytes_received: self.received,
            image_size: self.manifest.image_size,
        }
    }
}

fn heapless_string<const N: usize>(value: &str) -> Result<String<N>, UpdateManifestError> {
    let mut out = String::new();
    out.push_str(value)
        .map_err(|_| UpdateManifestError::StringTooLong)?;
    Ok(out)
}

pub fn update_manifest_digest(manifest: &UpdateManifest) -> Result<[u8; 32], UpdateManifestError> {
    let mut encoded = [0u8; MAX_UPDATE_MANIFEST_POSTCARD_LEN];
    let encoded =
        postcard::to_slice(manifest, &mut encoded).map_err(|_| UpdateManifestError::Encode)?;

    let mut h = Sha256::new();
    h.update(UPDATE_SIGNATURE_DOMAIN);
    h.update([0]);
    h.update(encoded);
    let digest = h.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(out)
}

pub fn verify_update_signature(
    manifest: &UpdateManifest,
    signature64: &[u8; 64],
    verifying_key_sec1: &[u8],
) -> Result<(), UpdateSignatureError> {
    let digest = update_manifest_digest(manifest).map_err(UpdateSignatureError::Manifest)?;
    let key = VerifyingKey::from_sec1_bytes(verifying_key_sec1)
        .map_err(|_| UpdateSignatureError::BadPublicKey)?;
    let sig = Signature::from_slice(signature64).map_err(|_| UpdateSignatureError::BadSignature)?;
    key.verify_prehash(&digest, &sig)
        .map_err(|_| UpdateSignatureError::VerifyFailed)
}

pub fn verify_update_bundle_signature(
    manifest: &UpdateManifest,
    signature64: &[u8; 64],
    bundled_pubkey_sec1: &[u8],
    trusted_pubkey_sha256: &[u8; 32],
) -> Result<(), UpdateSignatureError> {
    let bundled_hash = pubkey_sha256(bundled_pubkey_sec1);
    if &bundled_hash != trusted_pubkey_sha256 {
        return Err(UpdateSignatureError::UntrustedPublicKey);
    }
    if manifest.signing_pubkey_sha256 != bundled_hash {
        return Err(UpdateSignatureError::UntrustedPublicKey);
    }
    verify_update_signature(manifest, signature64, bundled_pubkey_sec1)
}

pub fn verify_update_image_digest(
    manifest: &UpdateManifest,
    image_sha256: &[u8; 32],
    image_size: u32,
) -> Result<(), UpdateSignatureError> {
    if manifest.image_size != image_size || &manifest.image_sha256 != image_sha256 {
        return Err(UpdateSignatureError::ImageHashMismatch);
    }
    Ok(())
}

pub fn verify_update_release_version(
    manifest: &UpdateManifest,
    current_release_version: u32,
) -> Result<(), UpdateSignatureError> {
    if manifest.release_version <= current_release_version {
        return Err(UpdateSignatureError::RollbackVersion);
    }
    Ok(())
}

pub fn verify_update_manifest_policy(
    manifest: &UpdateManifest,
    policy: &UpdateManifestPolicy<'_>,
) -> Result<(), UpdateManifestPolicyError> {
    if manifest.manifest_version != UPDATE_MANIFEST_VERSION
        || manifest.hardware_target.as_str() != policy.hardware_target
        || manifest.protocol_v != policy.protocol_v
        || manifest.image_size == 0
        || manifest.image_size > MAX_UPDATE_IMAGE_SIZE
        || !build_profile_allowed(
            policy.current_build_profile,
            manifest.build_profile.as_str(),
        )
    {
        return Err(UpdateManifestPolicyError::UnsupportedManifest);
    }

    verify_update_release_version(manifest, policy.current_release_version)
        .map_err(|_| UpdateManifestPolicyError::RollbackVersion)
}

fn build_profile_allowed(current: &str, candidate: &str) -> bool {
    if !is_supported_build_profile(current) || !is_supported_build_profile(candidate) {
        return false;
    }

    if current == UPDATE_BUILD_PROFILE_PRODUCTION {
        candidate == UPDATE_BUILD_PROFILE_PRODUCTION
    } else {
        true
    }
}

fn is_supported_build_profile(profile: &str) -> bool {
    matches!(
        profile,
        UPDATE_BUILD_PROFILE_DEV
            | UPDATE_BUILD_PROFILE_CHIP_SECURITY
            | UPDATE_BUILD_PROFILE_PRODUCTION
    )
}

pub fn pubkey_sha256(pubkey_sec1: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(pubkey_sec1);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manifest(pubkey_hash: [u8; 32]) -> UpdateManifest {
        UpdateManifest::new(
            7,
            3,
            Sha256::digest([1u8, 2, 3]).into(),
            pubkey_hash,
            "esp32s3-touch-lcd-1.47",
            "production",
            1,
            "0123456789abcdef0123456789abcdef01234567",
            "abcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap()
    }

    #[test]
    fn update_signature_checks_trust_anchor_before_signature_parsing() {
        let bundled_pubkey = b"not-a-sec1-public-key";
        let pubkey_hash = pubkey_sha256(bundled_pubkey);
        let manifest = test_manifest(pubkey_hash);
        let signature = [0u8; 64];

        verify_update_image_digest(&manifest, &Sha256::digest([1u8, 2, 3]).into(), 3).unwrap();

        let mut wrong_trust = pubkey_hash;
        wrong_trust[0] ^= 1;
        assert_eq!(
            verify_update_bundle_signature(&manifest, &signature, bundled_pubkey, &wrong_trust),
            Err(UpdateSignatureError::UntrustedPublicKey)
        );

        let manifest_for_other_key = test_manifest([7u8; 32]);
        assert_eq!(
            verify_update_bundle_signature(
                &manifest_for_other_key,
                &signature,
                bundled_pubkey,
                &pubkey_hash
            ),
            Err(UpdateSignatureError::UntrustedPublicKey)
        );

        assert_eq!(
            verify_update_bundle_signature(&manifest, &signature, bundled_pubkey, &pubkey_hash),
            Err(UpdateSignatureError::BadPublicKey)
        );
    }

    #[test]
    fn image_stream_verifier_checks_offsets_and_digest() {
        let image = [1u8, 2, 3, 4, 5];
        let manifest = test_manifest([9u8; 32]);
        let manifest = UpdateManifest::new(
            manifest.release_version,
            image.len() as u32,
            Sha256::digest(image).into(),
            manifest.signing_pubkey_sha256,
            manifest.hardware_target.as_str(),
            manifest.build_profile.as_str(),
            manifest.protocol_v,
            manifest.git_commit.as_str(),
            manifest.tx_types_rev.as_str(),
        )
        .unwrap();
        let mut verifier = UpdateImageVerifier::new(manifest);

        let status = verifier.append_chunk(0, &image[..2]).unwrap();
        assert_eq!(status.bytes_received, 2);
        assert_eq!(
            verifier.append_chunk(1, &image[2..3]),
            Err(UpdateImageStreamError::OffsetMismatch)
        );

        verifier.append_chunk(2, &image[2..]).unwrap();
        let status = verifier.finish().unwrap();
        assert!(!status.active);
        assert!(status.image_verified);
        assert_eq!(status.bytes_received, image.len() as u32);
    }

    #[test]
    fn image_stream_verifier_rejects_digest_mismatch() {
        let manifest = test_manifest([9u8; 32]);
        let mut verifier = UpdateImageVerifier::new(manifest);

        verifier.append_chunk(0, &[1, 2, 4]).unwrap();
        assert_eq!(
            verifier.finish(),
            Err(UpdateImageStreamError::ImageHashMismatch)
        );
    }

    #[test]
    fn release_version_must_advance_past_current_firmware() {
        let manifest = test_manifest([9u8; 32]);
        verify_update_release_version(&manifest, 6).unwrap();
        assert_eq!(
            verify_update_release_version(&manifest, 7),
            Err(UpdateSignatureError::RollbackVersion)
        );
        assert_eq!(
            verify_update_release_version(&manifest, 8),
            Err(UpdateSignatureError::RollbackVersion)
        );
    }

    #[test]
    fn update_manifest_policy_rejects_wrong_target_protocol_size_and_rollback() {
        let policy = UpdateManifestPolicy {
            current_release_version: 6,
            hardware_target: UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
            current_build_profile: UPDATE_BUILD_PROFILE_DEV,
            protocol_v: 1,
        };
        let manifest = test_manifest([9u8; 32]);
        verify_update_manifest_policy(&manifest, &policy).unwrap();

        let wrong_target = UpdateManifest::new(
            7,
            3,
            Sha256::digest([1u8, 2, 3]).into(),
            [9u8; 32],
            "esp32s3-other-board",
            UPDATE_BUILD_PROFILE_PRODUCTION,
            1,
            "0123456789abcdef0123456789abcdef01234567",
            "abcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap();
        assert_eq!(
            verify_update_manifest_policy(&wrong_target, &policy),
            Err(UpdateManifestPolicyError::UnsupportedManifest)
        );

        let wrong_protocol = UpdateManifest::new(
            7,
            3,
            Sha256::digest([1u8, 2, 3]).into(),
            [9u8; 32],
            UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
            UPDATE_BUILD_PROFILE_PRODUCTION,
            2,
            "0123456789abcdef0123456789abcdef01234567",
            "abcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap();
        assert_eq!(
            verify_update_manifest_policy(&wrong_protocol, &policy),
            Err(UpdateManifestPolicyError::UnsupportedManifest)
        );

        let empty_image = UpdateManifest::new(
            7,
            0,
            Sha256::digest([]).into(),
            [9u8; 32],
            UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
            UPDATE_BUILD_PROFILE_PRODUCTION,
            1,
            "0123456789abcdef0123456789abcdef01234567",
            "abcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap();
        assert_eq!(
            verify_update_manifest_policy(&empty_image, &policy),
            Err(UpdateManifestPolicyError::UnsupportedManifest)
        );

        let rollback_policy = UpdateManifestPolicy {
            current_release_version: 7,
            ..policy
        };
        assert_eq!(
            verify_update_manifest_policy(&manifest, &rollback_policy),
            Err(UpdateManifestPolicyError::RollbackVersion)
        );
    }

    #[test]
    fn production_manifest_policy_rejects_non_production_bundles() {
        let dev_manifest = UpdateManifest::new(
            7,
            3,
            Sha256::digest([1u8, 2, 3]).into(),
            [9u8; 32],
            UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
            UPDATE_BUILD_PROFILE_DEV,
            1,
            "0123456789abcdef0123456789abcdef01234567",
            "abcdef0123456789abcdef0123456789abcdef01",
        )
        .unwrap();
        let production_manifest = test_manifest([9u8; 32]);

        let dev_policy = UpdateManifestPolicy {
            current_release_version: 6,
            hardware_target: UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
            current_build_profile: UPDATE_BUILD_PROFILE_DEV,
            protocol_v: 1,
        };
        verify_update_manifest_policy(&dev_manifest, &dev_policy).unwrap();
        verify_update_manifest_policy(&production_manifest, &dev_policy).unwrap();

        let production_policy = UpdateManifestPolicy {
            current_build_profile: UPDATE_BUILD_PROFILE_PRODUCTION,
            ..dev_policy
        };
        assert_eq!(
            verify_update_manifest_policy(&dev_manifest, &production_policy),
            Err(UpdateManifestPolicyError::UnsupportedManifest)
        );
        verify_update_manifest_policy(&production_manifest, &production_policy).unwrap();
    }
}
