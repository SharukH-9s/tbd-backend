$baseUrl = "http://localhost:6751"
$endpoint = "$baseUrl/login"

# Using the user we just registered
$body = @{
    email    = "test_173057@example.com"
    password = "SafePassword123"
} | ConvertTo-Json

Write-Host "Logging in user: test_173057@example.com" -ForegroundColor Cyan
try {
    $response = Invoke-RestMethod -Uri $endpoint -Method Post -Body $body -ContentType "application/json"
    Write-Host "Success!" -ForegroundColor Green
    $response | Format-List
    
    # Extract tokens for display
    Write-Host "`nAccess Token: " -NoNewline
    Write-Host $response.data.access_token -ForegroundColor Yellow
    Write-Host "Refresh Token: " -NoNewline
    Write-Host $response.data.refresh_token -ForegroundColor Yellow
} catch {
    Write-Host "Failed!" -ForegroundColor Red
    $_.Exception.Message
    if ($_.ErrorDetails) {
        $_.ErrorDetails.Message
    }
}
