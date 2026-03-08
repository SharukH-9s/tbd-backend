$baseUrl = "http://localhost:6751"
$endpoint = "$baseUrl/register"

$timestamp = Get-Date -Format "HHmmss"
$testUser = "testuser_$timestamp"
$testEmail = "test_$timestamp@example.com"

$body = @{
    username = $testUser
    password = "SafePassword123"
    email    = $testEmail
    phone    = "1234567890"
} | ConvertTo-Json

Write-Host "Registering user: $testUser" -ForegroundColor Cyan
try {
    $response = Invoke-RestMethod -Uri $endpoint -Method Post -Body $body -ContentType "application/json"
    Write-Host "Success!" -ForegroundColor Green
    $response | Format-List
} catch {
    Write-Host "Failed!" -ForegroundColor Red
    $_.Exception.Message
    if ($_.ErrorDetails) {
        $_.ErrorDetails.Message
    }
}
