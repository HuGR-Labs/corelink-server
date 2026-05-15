output "role_arn" {
  description = "ARN of the CoreLink-side BYOK role. Customer-side trust policies reference this."
  value       = aws_iam_role.byok.arn
}

output "role_name" {
  description = "Role name."
  value       = aws_iam_role.byok.name
}
