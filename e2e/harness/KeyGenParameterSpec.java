package android.security.keystore;

import java.security.spec.AlgorithmParameterSpec;

/**
 * Host-JVM functional implementation of Android KeyGenParameterSpec so real
 * KeystoreKeyCipher and AiCredentialVault fluent Builders execute without NPE
 * against E2eAndroidShims's JCE AES-256-GCM "AndroidKeyStore" Provider.
 */
public final class KeyGenParameterSpec implements AlgorithmParameterSpec {
    private final String keystoreAlias;
    private final int purposes;
    private final int keySize;

    private KeyGenParameterSpec(String alias, int purposes, int keySize) {
        this.keystoreAlias = alias;
        this.purposes = purposes;
        this.keySize = keySize;
    }

    public String getKeystoreAlias() {
        return keystoreAlias;
    }

    public int getPurposes() {
        return purposes;
    }

    public int getKeySize() {
        return keySize;
    }

    public static final class Builder {
        private final String alias;
        private final int purposes;
        private int keySize = 256;

        public Builder(String keystoreAlias, int purposes) {
            if (keystoreAlias == null || keystoreAlias.isEmpty()) {
                throw new IllegalArgumentException("keystoreAlias must not be empty");
            }
            this.alias = keystoreAlias;
            this.purposes = purposes;
        }

        public Builder setKeySize(int keySize) {
            this.keySize = keySize;
            return this;
        }

        public Builder setBlockModes(String... blockModes) {
            return this;
        }

        public Builder setEncryptionPaddings(String... paddings) {
            return this;
        }

        public Builder setSignaturePaddings(String... paddings) {
            return this;
        }

        public Builder setDigests(String... digests) {
            return this;
        }

        public Builder setRandomizedEncryptionRequired(boolean required) {
            return this;
        }

        public Builder setUserAuthenticationRequired(boolean required) {
            return this;
        }

        public Builder setInvalidatedByBiometricEnrollment(boolean invalidateKey) {
            return this;
        }

        public Builder setUnlockedDeviceRequired(boolean unlockedDeviceRequired) {
            return this;
        }

        public Builder setIsStrongBoxBacked(boolean isStrongBoxBacked) {
            return this;
        }

        public KeyGenParameterSpec build() {
            return new KeyGenParameterSpec(alias, purposes, keySize);
        }
    }
}
