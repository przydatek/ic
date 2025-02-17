use crate::keygen::keygen_internal::KeyGenInternal;
use crate::{key_from_registry, CryptoComponentFatClient};
use ic_crypto_internal_csp::keygen::{forward_secure_key_id, public_key_hash_as_key_id};
use ic_crypto_internal_csp::types::CspPublicKey;
use ic_crypto_internal_csp::CryptoServiceProvider;
use ic_crypto_internal_types::encrypt::forward_secure::{
    CspFsEncryptionPop, CspFsEncryptionPublicKey,
};
use ic_interfaces::crypto::{KeyManager, Keygen};
use ic_protobuf::crypto::v1::NodePublicKeys;
use ic_registry_client::helper::crypto::CryptoRegistry;
use ic_types::crypto::{
    AlgorithmId, CommitteeMemberPublicKey, CryptoError, CryptoResult, KeyId, KeyPurpose,
    UserPublicKey,
};
use ic_types::RegistryVersion;
use std::convert::TryFrom;
use std::sync::Arc;

mod keygen_internal;

#[cfg(test)]
mod tests;

impl<C: CryptoServiceProvider> Keygen for CryptoComponentFatClient<C> {
    fn generate_user_keys_ed25519(&self) -> CryptoResult<(KeyId, UserPublicKey)> {
        KeyGenInternal::generate_user_keys_ed25519(&self.csp)
    }

    fn generate_committee_member_keys(&self) -> CryptoResult<(KeyId, CommitteeMemberPublicKey)> {
        KeyGenInternal::generate_committee_member_keys(&self.csp)
    }
}

impl<C: CryptoServiceProvider> KeyManager for CryptoComponentFatClient<C> {
    fn node_public_keys(&self) -> NodePublicKeys {
        self.csp.node_public_keys()
    }

    fn check_keys_with_registry(&self, registry_version: RegistryVersion) -> CryptoResult<()> {
        self.ensure_node_signing_key_material_is_set_up(registry_version)?;
        self.ensure_committee_signing_key_material_is_set_up(registry_version)?;
        self.ensure_dkg_dealing_encryption_key_material_is_set_up(registry_version)?;
        self.ensure_tls_key_material_is_set_up(registry_version)?;
        Ok(())
    }
}

// Helpers for implementing `KeyManager`-trait.
impl<C: CryptoServiceProvider> CryptoComponentFatClient<C> {
    fn ensure_node_signing_key_material_is_set_up(
        &self,
        registry_version: RegistryVersion,
    ) -> CryptoResult<()> {
        let pk_proto = key_from_registry(
            Arc::clone(&self.registry_client),
            self.node_id,
            KeyPurpose::NodeSigning,
            registry_version,
        )?;
        if AlgorithmId::from(pk_proto.algorithm) != AlgorithmId::Ed25519 {
            return Err(CryptoError::PublicKeyNotFound {
                node_id: self.node_id,
                key_purpose: KeyPurpose::NodeSigning,
                registry_version,
            });
        }
        let csp_key = CspPublicKey::try_from(pk_proto)?;
        let key_id = public_key_hash_as_key_id(&csp_key);
        if !self.csp.sks_contains(&key_id) {
            return Err(CryptoError::SecretKeyNotFound {
                algorithm: AlgorithmId::Ed25519,
                key_id,
            });
        }
        Ok(())
    }

    fn ensure_committee_signing_key_material_is_set_up(
        &self,
        registry_version: RegistryVersion,
    ) -> CryptoResult<()> {
        let pk_proto = key_from_registry(
            Arc::clone(&self.registry_client),
            self.node_id,
            KeyPurpose::CommitteeSigning,
            registry_version,
        )?;
        if AlgorithmId::from(pk_proto.algorithm) != AlgorithmId::MultiBls12_381 {
            return Err(CryptoError::PublicKeyNotFound {
                node_id: self.node_id,
                key_purpose: KeyPurpose::CommitteeSigning,
                registry_version,
            });
        }
        let csp_key = CspPublicKey::try_from(pk_proto)?;
        let key_id = public_key_hash_as_key_id(&csp_key);
        if !self.csp.sks_contains(&key_id) {
            return Err(CryptoError::SecretKeyNotFound {
                algorithm: AlgorithmId::MultiBls12_381,
                key_id,
            });
        }
        Ok(())
    }

    fn ensure_dkg_dealing_encryption_key_material_is_set_up(
        &self,
        registry_version: RegistryVersion,
    ) -> CryptoResult<()> {
        let pk_proto = key_from_registry(
            Arc::clone(&self.registry_client),
            self.node_id,
            KeyPurpose::DkgDealingEncryption,
            registry_version,
        )?;
        if AlgorithmId::from(pk_proto.algorithm) != AlgorithmId::Groth20_Bls12_381 {
            return Err(CryptoError::PublicKeyNotFound {
                node_id: self.node_id,
                key_purpose: KeyPurpose::CommitteeSigning,
                registry_version,
            });
        }
        let _csp_pop = CspFsEncryptionPop::try_from(&pk_proto).map_err(|e| {
            CryptoError::MalformedPublicKey {
                algorithm: AlgorithmId::Groth20_Bls12_381,
                key_bytes: None,
                internal_error: format!("{:?}", e),
            }
        })?;
        let csp_key = CspFsEncryptionPublicKey::try_from(pk_proto).map_err(|e| {
            CryptoError::MalformedPublicKey {
                algorithm: AlgorithmId::Groth20_Bls12_381,
                key_bytes: Some(e.key_bytes),
                internal_error: e.internal_error,
            }
        })?;
        let key_id = forward_secure_key_id(&csp_key);
        if !self.csp.sks_contains(&key_id) {
            return Err(CryptoError::SecretKeyNotFound {
                algorithm: AlgorithmId::Groth20_Bls12_381,
                key_id,
            });
        }
        Ok(())
    }

    fn ensure_tls_key_material_is_set_up(
        &self,
        registry_version: RegistryVersion,
    ) -> CryptoResult<()> {
        let x509_public_key_cert = self
            .registry_client
            .get_tls_certificate(self.node_id, registry_version)?
            .ok_or(CryptoError::TlsCertNotFound {
                node_id: self.node_id,
                registry_version,
            })?;
        if !self.csp.sks_contains_tls_key(&x509_public_key_cert) {
            return Err(CryptoError::TlsSecretKeyNotFound {
                certificate_der: x509_public_key_cert.certificate_der,
            });
        }
        Ok(())
    }
}
