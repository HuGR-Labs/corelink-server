# AWS S3 Object-Lock archive target.
#
# This module is deliberately unreferenced by an active environment root. A
# Security + Compliance-approved account, region, retention term, legal-hold
# process, and CloudTrail receipt source are prerequisites to applying it.
# See docs/operator/aws-s3-object-lock-archive-provisioning.md.

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
  object_arn_prefix = "${aws_s3_bucket.archive.arn}/${var.object_key_prefix}*"
  tags = merge(var.tags, {
    component  = "audit-object-lock-archive"
    managed_by = "terraform"
  })
}

resource "aws_s3_bucket" "archive" {
  bucket              = var.bucket_name
  object_lock_enabled = true

  # An archive with a destroyable target is not a retention control.
  lifecycle {
    prevent_destroy = true
  }

  tags = local.tags
}

resource "aws_s3_bucket_versioning" "archive" {
  bucket = aws_s3_bucket.archive.id

  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_object_lock_configuration" "archive" {
  bucket = aws_s3_bucket.archive.id

  rule {
    default_retention {
      mode = "COMPLIANCE"
      days = var.default_retention_days
    }
  }

  depends_on = [aws_s3_bucket_versioning.archive]
}

resource "aws_s3_bucket_public_access_block" "archive" {
  bucket = aws_s3_bucket.archive.id

  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_server_side_encryption_configuration" "archive" {
  bucket = aws_s3_bucket.archive.id

  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

data "aws_iam_policy_document" "writer_assume" {
  statement {
    effect  = "Allow"
    actions = ["sts:AssumeRole"]

    principals {
      type        = "AWS"
      identifiers = var.writer_principal_arns
    }
  }
}

data "aws_iam_policy_document" "writer" {
  statement {
    effect = "Allow"
    actions = [
      "s3:GetBucketObjectLockConfiguration",
      "s3:GetBucketLocation",
    ]
    resources = [aws_s3_bucket.archive.arn]
  }

  statement {
    effect = "Allow"
    actions = [
      "s3:GetObjectLegalHold",
      "s3:GetObjectRetention",
      "s3:PutObject",
    ]
    resources = [local.object_arn_prefix]
  }

  # The adapter may set only the module's Compliance retention term.
  statement {
    effect    = "Allow"
    actions   = ["s3:PutObjectRetention"]
    resources = [local.object_arn_prefix]

    condition {
      test     = "StringEquals"
      variable = "s3:object-lock-mode"
      values   = ["COMPLIANCE"]
    }

    condition {
      test     = "NumericGreaterThanEquals"
      variable = "s3:object-lock-remaining-retention-days"
      values   = [tostring(var.default_retention_days)]
    }

    condition {
      test     = "NumericLessThanEquals"
      variable = "s3:object-lock-remaining-retention-days"
      values   = [tostring(var.default_retention_days)]
    }
  }

  # The writer may set a legal hold while archiving, but cannot release one.
  # The S3 authorization key is evaluated against the requested ON/OFF state.
  statement {
    effect    = "Allow"
    actions   = ["s3:PutObjectLegalHold"]
    resources = [local.object_arn_prefix]

    condition {
      test     = "StringEquals"
      variable = "s3:object-lock-legal-hold"
      values   = ["ON"]
    }
  }
}

resource "aws_iam_role" "writer" {
  name               = var.writer_role_name
  assume_role_policy = data.aws_iam_policy_document.writer_assume.json
  tags               = local.tags
}

resource "aws_iam_role_policy" "writer" {
  name   = "${var.writer_role_name}-object-lock"
  role   = aws_iam_role.writer.id
  policy = data.aws_iam_policy_document.writer.json
}
