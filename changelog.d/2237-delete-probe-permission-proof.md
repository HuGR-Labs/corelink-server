### Security

- **Object Lock delete-denial capability checks now require independently verified permission evidence.** The optional AWS adapter rejects a denial unless the receipt binds the probe principal, bucket, key, version, and `s3:DeleteObjectVersion` action.
