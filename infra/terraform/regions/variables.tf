# Legacy variable declarations retained for documentation tooling.
# The drift workflow uses the four independent roots under this directory;
# this directory itself is not a Terraform root and must not receive init/plan.

variable "cf_account_id" {
  type      = string
  sensitive = true
}

variable "cf_zone_id" {
  type = string
}

variable "environment" {
  type    = string
  default = "staging"
}
