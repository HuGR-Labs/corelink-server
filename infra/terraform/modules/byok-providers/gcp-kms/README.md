# byok-providers / gcp-kms

CoreLink-side service account + Workload Identity Pool for a single GCP-KMS
BYOK customer. Customer grants the SA `roles/cloudkms.cryptoKeyEncrypterDecrypter`
on their CryptoKey out of band.

Variables: see `variables.tf`.

Outputs: `service_account_email`, `wif_pool_name`.

Operational flow mirrors the AWS module — see `../aws-kms/README.md`.
