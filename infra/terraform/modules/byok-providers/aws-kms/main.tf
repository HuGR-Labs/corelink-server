# byok-providers/aws-kms — CoreLink-side IAM role for AWS KMS BYOK.
#
# Provisions in CoreLink's OWN AWS account:
#   * IAM role that customer trust policies allow into.
#   * Inline policy granting kms:Encrypt / Decrypt / GenerateDataKey / DescribeKey
#     on the customer-owned CMK ARNs the customer hands us.
#
# Customer-side CMK + key policy is OUT OF SCOPE — customer manages.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.50"
    }
  }
}

locals {
  role_name = "corelink-byok-${var.customer_tenant_id}"
  tags = {
    tenant     = var.customer_tenant_id
    component  = "byok-aws-kms"
    managed_by = "terraform"
  }
}

data "aws_iam_policy_document" "assume" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "AWS"
      identifiers = [var.corelink_principal_arn]
    }

    condition {
      test     = "StringEquals"
      variable = "sts:ExternalId"
      values   = [var.external_id]
    }
  }
}

data "aws_iam_policy_document" "kms" {
  statement {
    effect = "Allow"
    actions = [
      "kms:Encrypt",
      "kms:Decrypt",
      "kms:GenerateDataKey",
      "kms:DescribeKey",
      "kms:ReEncrypt*",
    ]
    resources = var.customer_kms_key_arns
  }
}

resource "aws_iam_role" "byok" {
  name               = local.role_name
  assume_role_policy = data.aws_iam_policy_document.assume.json
  tags               = local.tags
}

resource "aws_iam_role_policy" "byok_kms" {
  name   = "${local.role_name}-kms"
  role   = aws_iam_role.byok.id
  policy = data.aws_iam_policy_document.kms.json
}
