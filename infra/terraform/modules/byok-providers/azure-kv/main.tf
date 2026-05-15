# byok-providers/azure-kv — CoreLink-side Azure AD app + SP for Key Vault BYOK.
#
# Provisions in CoreLink's OWN Azure AD tenant:
#   * App registration (CoreLink-owned identity that customer will trust).
#   * Service Principal in the customer's tenant via cross-tenant-app
#     consent (customer admin grants consent out of band).
#
# Customer-side Key Vault + key permission assignment is OUT OF SCOPE.

terraform {
  required_version = ">= 1.7.0, < 2.0.0"

  required_providers {
    azuread = {
      source  = "hashicorp/azuread"
      version = "~> 2.50"
    }
  }
}

locals {
  app_display_name = "corelink-byok-${var.customer_tenant_id}"
}

resource "azuread_application" "byok" {
  display_name = local.app_display_name

  # Multi-tenant — customer will consent in their AAD tenant.
  sign_in_audience = "AzureADMultipleOrgs"

  required_resource_access {
    # Azure Key Vault resource ID.
    resource_app_id = "cfa8b339-82a2-471a-a3c9-0fc0be7a4093"

    resource_access {
      # user_impersonation scope — required to acquire AKV tokens.
      id   = "f53da476-18e3-4152-8e01-aec403e6edc0"
      type = "Scope"
    }
  }
}

resource "azuread_service_principal" "byok" {
  client_id                    = azuread_application.byok.client_id
  app_role_assignment_required = false
}

resource "azuread_application_password" "byok" {
  application_id    = azuread_application.byok.id
  display_name      = "corelink-byok-${var.customer_tenant_id}-secret"
  end_date_relative = "8760h" # 365 days — rotated by RB-SECRETS-DRIFT / BYOK rotation runbook.
}
