output "archive_bucket" {
  description = "Non-secret bucket identifier for the eventual Object-Lock adapter configuration."
  value       = aws_s3_bucket.archive.id
}

output "archive_region" {
  description = "AWS region returned by the provider for residency evidence."
  value       = aws_s3_bucket.archive.region
}

output "writer_role_arn" {
  description = "Dedicated role ARN for the archive writer workload identity."
  value       = aws_iam_role.writer.arn
}

output "object_lock_mode" {
  description = "The configured default retention mode; runtime must still read it back from S3."
  value       = "COMPLIANCE"
}
