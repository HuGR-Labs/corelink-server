[CmdletBinding()]
param(
    [switch] $SelfTest,
    [string] $Repository,
    [string] $WorkflowName,
    [string] $CommitSha,
    [string] $RunId,
    [string] $RunUrl,
    [string] $ExpectedOperation
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ApprovedOperation = 'authenticode-credential-binding-metadata-only'
$script:ApprovedTimestampPolicy = 'DigiCert RFC 3161 http://timestamp.digicert.com; SHA-256 file and timestamp digests; not invoked'
$script:ReceiptFields = @(
    'schema_version', 'evidence_type', 'repository', 'workflow', 'commit_sha',
    'run_id', 'run_url', 'observed_at', 'actor_role', 'approved_operation',
    'certificate', 'chain_revocation', 'timestamp_policy', 'result',
    'artifact_signature', 'final_byte_verification', 'renewal'
)
$script:CertificateReceiptFields = @(
    'issuer_match', 'subject_match', 'sha256_fingerprint', 'valid_from', 'expires_at'
)
$script:ChainReceiptFields = @('status', 'mode')
$script:RenewalReceiptFields = @('owner', 'date')

function ConvertTo-NormalizedName {
    param([Parameter(Mandatory)][string] $Value)

    ($Value -replace '\s*([,=])\s*', '$1').Trim().ToUpperInvariant()
}

function ConvertTo-UtcSecond {
    param([Parameter(Mandatory)][DateTimeOffset] $Value)

    $Value.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
}

function Assert-ReadinessClaims {
    param(
        [Parameter(Mandatory)] $Metadata,
        [Parameter(Mandatory)] [string] $ExpectedIssuer,
        [Parameter(Mandatory)] [string] $ExpectedSubject,
        [Parameter(Mandatory)] [string] $ExpectedFingerprint,
        [Parameter(Mandatory)] [string] $ExpectedNotBefore,
        [Parameter(Mandatory)] [string] $ExpectedExpiresAt,
        [Parameter(Mandatory)] [string] $TimestampPolicy,
        [Parameter(Mandatory)] [string] $Operation,
        [Parameter(Mandatory)] [string] $RenewalOwner,
        [Parameter(Mandatory)] [string] $RenewalDate,
        [Parameter(Mandatory)] [DateTimeOffset] $ObservedAt
    )

    if ($Operation -cne $script:ApprovedOperation) { throw 'unapproved_operation' }
    if ($TimestampPolicy -cne $script:ApprovedTimestampPolicy) { throw 'unapproved_timestamp_policy' }
    foreach ($field in @('Issuer', 'Subject', 'Fingerprint', 'NotBefore', 'ExpiresAt', 'HasPrivateKey', 'ChainRevocationPassed')) {
        if ($null -eq $Metadata.PSObject.Properties[$field]) { throw 'certificate_metadata_missing' }
    }
    foreach ($field in @('ExpectedIssuer', 'ExpectedSubject', 'ExpectedFingerprint', 'ExpectedNotBefore', 'ExpectedExpiresAt', 'RenewalOwner', 'RenewalDate')) {
        if ([string]::IsNullOrWhiteSpace((Get-Variable -Name $field -ValueOnly))) { throw 'approved_metadata_missing' }
    }
    if ($Metadata.HasPrivateKey -isnot [bool] -or -not $Metadata.HasPrivateKey) { throw 'private_key_association_missing' }
    if ($Metadata.ChainRevocationPassed -isnot [bool] -or -not $Metadata.ChainRevocationPassed) { throw 'chain_revocation_failed' }

    if ((ConvertTo-NormalizedName ([string] $Metadata.Issuer)) -cne (ConvertTo-NormalizedName $ExpectedIssuer)) {
        throw 'issuer_mismatch'
    }
    if ((ConvertTo-NormalizedName ([string] $Metadata.Subject)) -cne (ConvertTo-NormalizedName $ExpectedSubject)) {
        throw 'subject_mismatch'
    }

    $actualFingerprint = ([string] $Metadata.Fingerprint -replace '[:\s-]', '').ToUpperInvariant()
    $expectedFingerprint = ($ExpectedFingerprint -replace '[:\s-]', '').ToUpperInvariant()
    if ($actualFingerprint -notmatch '^[0-9A-F]{64}$' -or $actualFingerprint -cne $expectedFingerprint) {
        throw 'fingerprint_mismatch'
    }

    $notBefore = [DateTimeOffset]::Parse($Metadata.NotBefore, [Globalization.CultureInfo]::InvariantCulture)
    $expiresAt = [DateTimeOffset]::Parse($Metadata.ExpiresAt, [Globalization.CultureInfo]::InvariantCulture)
    $approvedNotBefore = [DateTimeOffset]::Parse($ExpectedNotBefore, [Globalization.CultureInfo]::InvariantCulture)
    $approvedExpiresAt = [DateTimeOffset]::Parse($ExpectedExpiresAt, [Globalization.CultureInfo]::InvariantCulture)
    if ((ConvertTo-UtcSecond $notBefore) -cne (ConvertTo-UtcSecond $approvedNotBefore) -or
        (ConvertTo-UtcSecond $expiresAt) -cne (ConvertTo-UtcSecond $approvedExpiresAt)) {
        throw 'validity_mismatch'
    }
    if ($notBefore -gt $ObservedAt -or $expiresAt -le $ObservedAt) { throw 'certificate_outside_validity' }
    if ($expiresAt -le $notBefore) { throw 'invalid_certificate_validity' }
    if ($RenewalDate -notmatch '^\d{4}-\d{2}-\d{2}$') { throw 'renewal_date_invalid' }
    $renewalDay = [DateTime]::ParseExact(
        $RenewalDate, 'yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]::AssumeUniversal
    )
    if ($renewalDay.Date -le $ObservedAt.UtcDateTime.Date) {
        throw 'renewal_date_not_future'
    }
    if ($RenewalOwner -cne 'Release Engineering') { throw 'renewal_owner_not_approved_role' }

    return [PSCustomObject]@{
        issuer_match = $true
        subject_match = $true
        sha256_fingerprint = $actualFingerprint
        valid_from = ConvertTo-UtcSecond $notBefore
        expires_at = ConvertTo-UtcSecond $expiresAt
    }
}

function Assert-ReceiptShape {
    param([Parameter(Mandatory)] $Receipt)

    $actualFields = @($Receipt.PSObject.Properties.Name)
    if (($actualFields -join '|') -cne ($script:ReceiptFields -join '|')) { throw 'receipt_field_drift' }
    if ($Receipt.result -cne 'ready' -or $Receipt.evidence_type -cne 'readiness') { throw 'receipt_result_invalid' }
    if ($Receipt.artifact_signature -cne 'not_claimed' -or $Receipt.final_byte_verification -cne 'not_claimed') {
        throw 'receipt_artifact_claim_invalid'
    }
    foreach ($entry in @(
        @{ Value = $Receipt.certificate; Fields = $script:CertificateReceiptFields }
        @{ Value = $Receipt.chain_revocation; Fields = $script:ChainReceiptFields }
        @{ Value = $Receipt.renewal; Fields = $script:RenewalReceiptFields }
    )) {
        if (($entry.Value.PSObject.Properties.Name -join '|') -cne ($entry.Fields -join '|')) {
            throw 'receipt_nested_field_drift'
        }
    }
    if (($Receipt | ConvertTo-Json -Depth 8 -Compress) -match '(?i)(password|private.?key|token|secret.?value|pfx.?bytes|certificate.?bytes|@)') {
        throw 'receipt_sensitive_field'
    }
}

function Invoke-SyntheticClaimsCheck {
    param([Parameter(Mandatory)] $Metadata, [Parameter(Mandatory)] [hashtable] $Values)

    $parameters = @{ Metadata = $Metadata }
    foreach ($entry in $Values.GetEnumerator()) { $parameters[$entry.Key] = $entry.Value }
    Assert-ReadinessClaims @parameters
}

function Invoke-CredentiallessSelfTest {
    $now = [DateTimeOffset]::Parse('2026-09-25T12:00:00Z')
    $expected = @{
        ExpectedIssuer = 'CN=Example Issuing CA,O=Example CA,C=US'
        ExpectedSubject = 'CN=CoreLink Windows Signing,O=Example Org,C=US'
        ExpectedFingerprint = ('A' * 64)
        ExpectedNotBefore = '2026-01-01T00:00:00Z'
        ExpectedExpiresAt = '2027-01-01T00:00:00Z'
        TimestampPolicy = $script:ApprovedTimestampPolicy
        Operation = $script:ApprovedOperation
        RenewalOwner = 'Release Engineering'
        RenewalDate = '2026-12-01'
        ObservedAt = $now
    }
    $metadata = [PSCustomObject]@{
        Issuer = 'CN=Example Issuing CA,O=Example CA,C=US'
        Subject = 'CN=CoreLink Windows Signing,O=Example Org,C=US'
        Fingerprint = ('A' * 64)
        NotBefore = '2026-01-01T00:00:00Z'
        ExpiresAt = '2027-01-01T00:00:00Z'
        HasPrivateKey = $true
        ChainRevocationPassed = $true
    }
    [void](Invoke-SyntheticClaimsCheck -Metadata $metadata -Values $expected)

    $cases = @(
        @{ Label = 'missing issuer'; Change = { param($m) $m.Issuer = '' } }
        @{ Label = 'mismatched subject'; Change = { param($m) $m.Subject = 'CN=Wrong,O=Example Org,C=US' } }
        @{ Label = 'reordered issuer'; Change = { param($m) $m.Issuer = 'C=US,O=Example CA,CN=Example Issuing CA' } }
        @{ Label = 'reordered subject'; Change = { param($m) $m.Subject = 'C=US,O=Example Org,CN=CoreLink Windows Signing' } }
        @{ Label = 'mismatched fingerprint'; Change = { param($m) $m.Fingerprint = ('B' * 64) } }
        @{ Label = 'expired certificate'; Change = { param($m) $m.ExpiresAt = '2026-09-24T00:00:00Z' } }
        @{ Label = 'not-yet-valid certificate'; Change = { param($m) $m.NotBefore = '2026-09-26T00:00:00Z' } }
        @{ Label = 'missing private key'; Change = { param($m) $m.HasPrivateKey = $false } }
        @{ Label = 'failed chain/revocation'; Change = { param($m) $m.ChainRevocationPassed = $false } }
    )
    foreach ($case in $cases) {
        $mutated = $metadata.PSObject.Copy()
        & $case.Change $mutated
        $rejected = $false
        try { [void](Invoke-SyntheticClaimsCheck -Metadata $mutated -Values $expected) } catch { $rejected = $true }
        if (-not $rejected) { throw "self-test failed: $($case.Label) was accepted" }
    }
    foreach ($case in @(
        @{ Label = 'unapproved operation'; Values = @{ Operation = 'sign-and-timestamp' } }
        @{ Label = 'unapproved timestamp policy'; Values = @{ TimestampPolicy = 'unapproved policy' } }
        @{ Label = 'missing renewal owner'; Values = @{ RenewalOwner = '' } }
        @{ Label = 'missing renewal date'; Values = @{ RenewalDate = '' } }
    )) {
        $overrides = @{} + $expected + $case.Values
        $rejected = $false
        try { [void](Invoke-SyntheticClaimsCheck -Metadata $metadata -Values $overrides) } catch { $rejected = $true }
        if (-not $rejected) { throw "self-test failed: $($case.Label) was accepted" }
    }

    $validIdentity = Invoke-SyntheticClaimsCheck -Metadata $metadata -Values $expected
    $receipt = [PSCustomObject][ordered]@{
        schema_version = 1
        evidence_type = 'readiness'
        repository = 'HuGR-dev/corelink-server'
        workflow = 'issue-2586-windows-readiness'
        commit_sha = ('a' * 40)
        run_id = '123456789'
        run_url = 'https://github.com/HuGR-dev/corelink-server/actions/runs/123456789'
        observed_at = ConvertTo-UtcSecond $now
        actor_role = 'authorized operator through protected environment approval'
        approved_operation = $script:ApprovedOperation
        certificate = $validIdentity
        chain_revocation = [PSCustomObject]@{ status = 'passed'; mode = 'online_entire_chain' }
        timestamp_policy = $script:ApprovedTimestampPolicy
        result = 'ready'
        artifact_signature = 'not_claimed'
        final_byte_verification = 'not_claimed'
        renewal = [PSCustomObject]@{ owner = 'Release Engineering'; date = '2026-12-01' }
    }
    Assert-ReceiptShape $receipt
    if (@($receipt.PSObject.Properties.Name) -join '|' -cne ($script:ReceiptFields -join '|')) {
        throw 'self-test failed: receipt field drift was accepted'
    }
    $driftedReceipt = $receipt | Select-Object *
    $driftedReceipt | Add-Member -NotePropertyName unexpected_field -NotePropertyValue 'synthetic'
    $driftRejected = $false
    try { Assert-ReceiptShape $driftedReceipt } catch { $driftRejected = $true }
    if (-not $driftRejected) { throw 'self-test failed: receipt field drift was accepted' }
    $dnReceipt = $receipt | Select-Object *
    $dnReceipt.certificate = $receipt.certificate | Select-Object *
    $dnReceipt.certificate | Add-Member -NotePropertyName subject -NotePropertyValue 'synthetic personal name'
    $dnRejected = $false
    try { Assert-ReceiptShape $dnReceipt } catch { $dnRejected = $true }
    if (-not $dnRejected) { throw 'self-test failed: raw distinguished name was accepted in the receipt' }
    $secretLikeReceipt = $receipt | Select-Object *
    $secretLikeReceipt.actor_role = 'operator@example.invalid'
    $secretRejected = $false
    try { Assert-ReceiptShape $secretLikeReceipt } catch { $secretRejected = $true }
    if (-not $secretRejected) { throw 'self-test failed: secret-like or personal output was accepted' }
    Write-Output 'issue-2586 credentialless PowerShell contract passed'
}

if ($SelfTest) {
    Invoke-CredentiallessSelfTest
    exit 0
}

$pfxBytes = $null
$certificate = $null
$privateKey = $null
$securePassword = $null
$chain = $null
$failure = 'preflight_failed'
try {
    if ($ExpectedOperation -cne $script:ApprovedOperation) { throw 'unapproved_operation' }
    foreach ($name in @(
        'WINDOWS_CODE_SIGNING_CERT', 'WINDOWS_CODE_SIGNING_PASSWORD',
        'WINDOWS_CODE_SIGNING_FINGERPRINT', 'WINDOWS_CODE_SIGNING_SUBJECT',
        'WINDOWS_CODE_SIGNING_ISSUER', 'WINDOWS_CODE_SIGNING_NOT_BEFORE',
        'WINDOWS_CODE_SIGNING_EXPIRES_AT', 'WINDOWS_SIGNING_RENEWAL_OWNER',
        'WINDOWS_SIGNING_RENEWAL_DATE', 'REPOSITORY', 'WORKFLOW_NAME',
        'COMMIT_SHA', 'RUN_ID', 'RUN_URL', 'GITHUB_RUN_ID', 'GITHUB_REPOSITORY',
        'GITHUB_SHA', 'GITHUB_WORKFLOW', 'GITHUB_STEP_SUMMARY'
    )) {
        if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) { throw 'required_input_missing' }
    }
    $expectedRunUrl = "https://github.com/$Repository/actions/runs/$RunId"
    if ($Repository -cne $env:GITHUB_REPOSITORY -or $Repository -cne 'HuGR-dev/corelink-server' -or
        $WorkflowName -cne $env:GITHUB_WORKFLOW -or $CommitSha -cne $env:GITHUB_SHA -or
        $RunId -cne $env:GITHUB_RUN_ID -or $RunUrl -cne $expectedRunUrl) {
        throw 'run_identity_mismatch'
    }
    if ($CommitSha -notmatch '^[0-9a-f]{40}$' -or $RunId -notmatch '^[0-9]+$') { throw 'run_identity_invalid' }

    $pfxBytes = [Convert]::FromBase64String($env:WINDOWS_CODE_SIGNING_CERT)
    if ($pfxBytes.Length -lt 1) { throw 'certificate_input_invalid' }
    $securePassword = ConvertTo-SecureString -String $env:WINDOWS_CODE_SIGNING_PASSWORD -AsPlainText -Force
    try {
        $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
            $pfxBytes,
            $securePassword,
            [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
        )
    } catch { throw 'certificate_password_binding_failed' }
    if (-not $certificate.HasPrivateKey) { throw 'private_key_association_missing' }
    $privateKey = [System.Security.Cryptography.X509Certificates.RSACertificateExtensions]::GetRSAPrivateKey($certificate)
    if ($null -eq $privateKey) {
        $privateKey = [System.Security.Cryptography.X509Certificates.ECDsaCertificateExtensions]::GetECDsaPrivateKey($certificate)
    }
    if ($null -eq $privateKey) { throw 'private_key_association_missing' }

    $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
    $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::Online
    $chain.ChainPolicy.RevocationFlag = [System.Security.Cryptography.X509Certificates.X509RevocationFlag]::EntireChain
    $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::NoFlag
    $chain.ChainPolicy.UrlRetrievalTimeout = [TimeSpan]::FromSeconds(20)
    $chainPassed = $chain.Build($certificate)

    $observedAt = [DateTimeOffset]::UtcNow
    $fingerprint = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($certificate.RawData))
    $metadata = [PSCustomObject]@{
        Issuer = $certificate.IssuerName.Name
        Subject = $certificate.SubjectName.Name
        Fingerprint = $fingerprint
        NotBefore = $certificate.NotBefore.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
        ExpiresAt = $certificate.NotAfter.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
        HasPrivateKey = $certificate.HasPrivateKey
        ChainRevocationPassed = [bool] $chainPassed
    }
    $validIdentity = Assert-ReadinessClaims -Metadata $metadata `
        -ExpectedIssuer $env:WINDOWS_CODE_SIGNING_ISSUER `
        -ExpectedSubject $env:WINDOWS_CODE_SIGNING_SUBJECT `
        -ExpectedFingerprint $env:WINDOWS_CODE_SIGNING_FINGERPRINT `
        -ExpectedNotBefore $env:WINDOWS_CODE_SIGNING_NOT_BEFORE `
        -ExpectedExpiresAt $env:WINDOWS_CODE_SIGNING_EXPIRES_AT `
        -TimestampPolicy $script:ApprovedTimestampPolicy `
        -Operation $ExpectedOperation `
        -RenewalOwner $env:WINDOWS_SIGNING_RENEWAL_OWNER `
        -RenewalDate $env:WINDOWS_SIGNING_RENEWAL_DATE `
        -ObservedAt $observedAt

    if ($chain.ChainStatus.Count -ne 0) { throw 'chain_revocation_failed' }
    $receipt = [PSCustomObject][ordered]@{
        schema_version = 1
        evidence_type = 'readiness'
        repository = $Repository
        workflow = $WorkflowName
        commit_sha = $CommitSha
        run_id = $RunId
        run_url = $RunUrl
        observed_at = ConvertTo-UtcSecond $observedAt
        actor_role = 'authorized operator through protected environment approval'
        approved_operation = $ExpectedOperation
        certificate = $validIdentity
        chain_revocation = [PSCustomObject]@{ status = 'passed'; mode = 'online_entire_chain' }
        timestamp_policy = $script:ApprovedTimestampPolicy
        result = 'ready'
        artifact_signature = 'not_claimed'
        final_byte_verification = 'not_claimed'
        renewal = [PSCustomObject]@{
            owner = $env:WINDOWS_SIGNING_RENEWAL_OWNER
            date = $env:WINDOWS_SIGNING_RENEWAL_DATE
        }
    }
    Assert-ReceiptShape $receipt
    $json = $receipt | ConvertTo-Json -Depth 8
    $summary = '```json' + [Environment]::NewLine + $json + [Environment]::NewLine + '```'
    Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $summary -Encoding utf8
    $failure = $null
} catch {
    if ($_.Exception.Message -match '^[a-z_]+$') { $failure = $_.Exception.Message }
} finally {
    # The PFX is decoded only in this process and imported with EphemeralKeySet;
    # no certificate or password file is created. Hosted-runner process memory is
    # discarded on cancellation, while ordinary success/failure clears it here.
    if ($null -ne $privateKey) { $privateKey.Dispose() }
    if ($null -ne $certificate) { $certificate.Dispose() }
    if ($null -ne $chain) { $chain.Dispose() }
    if ($null -ne $securePassword) { $securePassword.Dispose() }
    if ($null -ne $pfxBytes) { [Array]::Clear($pfxBytes, 0, $pfxBytes.Length) }
    foreach ($name in @(
        'WINDOWS_CODE_SIGNING_CERT', 'WINDOWS_CODE_SIGNING_PASSWORD',
        'WINDOWS_CODE_SIGNING_FINGERPRINT', 'WINDOWS_CODE_SIGNING_SUBJECT',
        'WINDOWS_CODE_SIGNING_ISSUER', 'WINDOWS_CODE_SIGNING_NOT_BEFORE',
        'WINDOWS_CODE_SIGNING_EXPIRES_AT', 'WINDOWS_SIGNING_RENEWAL_OWNER',
        'WINDOWS_SIGNING_RENEWAL_DATE'
    )) {
        [Environment]::SetEnvironmentVariable($name, $null)
    }
}

if ($null -ne $failure) {
    [Console]::Error.WriteLine("::error::Windows credential readiness failed ($failure); no receipt was emitted.")
    exit 1
}
